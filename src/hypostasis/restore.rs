//! Restores (research R7): the library DB, the FlexAI workbooks, and the palaces. Each
//! runs only when its item is missing, never replaces existing data (FR-010b), lands
//! atomically (FR-010c), and is independent: setup tries them all (FR-016).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::system::action::{Check, Step};
use crate::system::{Cmd, PathKind, System, run_ok};

use super::Context;
use super::services::{self, Kind};

fn unreachable_folder(dir: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "backup folder {} is not reachable: mount {}, then run 'local-machine hypostasis setup'",
        dir.display(),
        dir.display()
    )
}

/// Whether a backup folder can be read.
pub fn reachable(sys: &dyn System, dir: &Path) -> bool {
    sys.path_kind(dir) == PathKind::Dir
}

/// The newest `<stem>-YYYY-MM-DD.db` in `dir`, else an undated `<stem>.db`.
pub fn newest_backup(sys: &dyn System, dir: &Path, stem: &str) -> Option<PathBuf> {
    let names = sys.list_dir(dir);
    let prefix = format!("{stem}-");
    let dated = names
        .iter()
        .filter(|n| n.starts_with(&prefix) && n.ends_with(".db"))
        .max()
        .cloned();
    let undated = format!("{stem}.db");
    dated
        .or_else(|| names.contains(&undated).then_some(undated))
        .map(|n| dir.join(n))
}

fn partial(target: &Path) -> PathBuf {
    let mut s = target.as_os_str().to_owned();
    s.push(".partial");
    PathBuf::from(s)
}

/// Copy a SQLite backup to `<target>.partial`, check it, and rename it into place.
fn restore_db(sys: &dyn System, backup: &Path, target: &Path) -> Result<()> {
    let tmp = partial(target);
    let result = (|| {
        if let Some(parent) = target.parent()
            && sys.path_kind(parent) == PathKind::Missing
        {
            sys.make_dir_all(parent)?;
        }
        sys.copy_file(backup, &tmp)?;
        let out = run_ok(
            sys,
            &Cmd::new("sqlite3").args([&tmp.display().to_string(), "PRAGMA quick_check"]),
        )?;
        if out.stdout.trim() != "ok" {
            bail!(
                "{} failed its check: {}",
                backup.display(),
                out.stdout.trim()
            );
        }
        sys.rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = sys.remove_user_file(&tmp);
    }
    result
}

// ---------------------------------------------------------------------------------------

/// `restore rpg-library`: the library DB, and `user_data.db` beside it when backed up.
pub struct RpgLibrary<'a> {
    pub ctx: &'a Context,
}

impl Step for RpgLibrary<'_> {
    fn name(&self) -> &str {
        "restore rpg-library"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let Some(a) = self.ctx.authority(sys)? else {
            return Ok(Check::NeedsChange("needs the authority file".into()));
        };
        Ok(if sys.path_kind(&a.rpg_library_db) == PathKind::File {
            Check::Done
        } else {
            Check::NeedsChange(format!("{} does not exist", a.rpg_library_db.display()))
        })
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        let dir = &self.ctx.cfg.backups.rpg_library;
        if !reachable(sys, dir) {
            return Err(unreachable_folder(dir));
        }
        let Some(db) = newest_backup(sys, dir, "rpg_library") else {
            bail!("no rpg_library backup in {}", dir.display());
        };
        restore_db(sys, &db, &a.rpg_library_db)?;
        let user_target = a
            .rpg_library_db
            .parent()
            .map(|p| p.join("user_data.db"))
            .unwrap_or_else(|| PathBuf::from("user_data.db"));
        if sys.path_kind(&user_target) == PathKind::Missing
            && let Some(user) = newest_backup(sys, dir, "user_data")
        {
            restore_db(sys, &user, &user_target)?;
        }
        Ok(())
    }

    fn continue_on_failure(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------

/// What an app's `--check` reported.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AppCheck {
    pub ok: bool,
    pub missing: Vec<String>,
    pub lines: Vec<String>,
}

/// `<svenv>/bin/python <mytools>/<app>/app.py --data-dir <dir> --check` (mytools#101).
/// The FlexAI data check runs `app.py --check` in the services venv; before setup has made
/// that venv there is nothing to run it with, and the data is not yet known to be missing.
pub fn services_venv_missing(ctx: &Context, sys: &dyn System) -> bool {
    sys.path_kind(&ctx.cfg.services_venv) != PathKind::Dir
}

pub fn flexai_check(ctx: &Context, sys: &dyn System, kind: Kind) -> AppCheck {
    let data_dir = match kind {
        Kind::FlexSocial => &ctx.cfg.flexai_social.data_dir,
        _ => &ctx.cfg.flexai_combat.data_dir,
    };
    let cmd = Cmd::new(&services::services_python(ctx).display().to_string()).args([
        services::app_dir(ctx, kind)
            .join("app.py")
            .display()
            .to_string(),
        "--data-dir".to_string(),
        data_dir.display().to_string(),
        "--check".to_string(),
    ]);
    match sys.run(&cmd) {
        Err(e) => AppCheck {
            ok: false,
            missing: Vec::new(),
            lines: vec![format!("cannot run the check: {e:#}")],
        },
        Ok(out) => {
            let lines: Vec<String> = out
                .stdout
                .lines()
                .chain(out.stderr.lines())
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            let missing = out
                .stdout
                .lines()
                .filter_map(|l| l.trim().strip_prefix("missing: "))
                .map(|f| f.trim().to_string())
                .collect();
            AppCheck {
                ok: out.success(),
                missing,
                lines,
            }
        }
    }
}

/// `restore <service> data`: copy only the files `--check` reports missing.
pub struct FlexData<'a> {
    pub ctx: &'a Context,
    pub kind: Kind,
    name: String,
}

impl<'a> FlexData<'a> {
    pub fn new(ctx: &'a Context, kind: Kind) -> Self {
        FlexData {
            ctx,
            kind,
            name: format!("restore {} data", kind.name()),
        }
    }

    fn data_dir(&self) -> &Path {
        match self.kind {
            Kind::FlexSocial => &self.ctx.cfg.flexai_social.data_dir,
            _ => &self.ctx.cfg.flexai_combat.data_dir,
        }
    }

    fn backup_dir(&self) -> &Path {
        match self.kind {
            Kind::FlexSocial => &self.ctx.cfg.backups.flexai_social,
            _ => &self.ctx.cfg.backups.flexai_combat,
        }
    }
}

impl Step for FlexData<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        if services_venv_missing(self.ctx, sys) {
            return Ok(Check::NeedsChange(format!(
                "checked once setup has made {}",
                self.ctx.cfg.services_venv.display()
            )));
        }
        let c = flexai_check(self.ctx, sys, self.kind);
        Ok(if c.ok {
            Check::Done
        } else {
            Check::NeedsChange(c.lines.join("; "))
        })
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let first = flexai_check(self.ctx, sys, self.kind);
        if first.ok {
            return Ok(());
        }
        if first.missing.is_empty() {
            bail!(
                "{} cannot be fixed by a restore (existing files are never replaced): {}",
                self.data_dir().display(),
                first.lines.join("; ")
            );
        }
        let backup = self.backup_dir();
        if !reachable(sys, backup) {
            return Err(unreachable_folder(backup));
        }
        let data_dir = self.data_dir().to_path_buf();
        let mut added: Vec<PathBuf> = Vec::new();
        let result = (|| {
            if sys.path_kind(&data_dir) == PathKind::Missing {
                sys.make_dir_all(&data_dir)?;
            }
            for file in &first.missing {
                if file.contains('/') {
                    bail!("the check named '{file}', which is not a plain file name");
                }
                let src = backup.join(file);
                let target = data_dir.join(file);
                if sys.path_kind(&target) != PathKind::Missing {
                    continue;
                }
                if sys.path_kind(&src) != PathKind::File {
                    bail!("{file} is not in {}", backup.display());
                }
                let tmp = partial(&target);
                if let Err(e) = sys
                    .copy_file(&src, &tmp)
                    .and_then(|_| sys.rename(&tmp, &target))
                {
                    let _ = sys.remove_user_file(&tmp);
                    return Err(e);
                }
                added.push(target);
            }
            let again = flexai_check(self.ctx, sys, self.kind);
            if !again.ok {
                bail!(
                    "still not usable after the restore: {}",
                    again.lines.join("; ")
                );
            }
            Ok(())
        })();
        if result.is_err() {
            for f in &added {
                let _ = sys.remove_user_file(f);
            }
        }
        result
    }

    fn continue_on_failure(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------

/// Whether `mneme mp status` reports a store for the campaign.
pub fn palace_present(ctx: &Context, sys: &dyn System, campaign: &str) -> Result<bool> {
    let Some(a) = ctx.authority(sys)? else {
        return Ok(false);
    };
    let out = sys.run(&Cmd::new(&a.bin("mneme").display().to_string()).args([
        "mp",
        "status",
        campaign,
        "--no-proposals",
        "--no-fetch",
        "--config",
        &a.path.display().to_string(),
    ]));
    let Ok(out) = out else {
        return Ok(false);
    };
    Ok(store_present(&out.stdout, campaign))
}

/// A `store` row for the campaign whose state is not `missing_config`.
pub fn store_present(stdout: &str, campaign: &str) -> bool {
    stdout.lines().any(|l| {
        let t: Vec<&str> = l.split_whitespace().collect();
        t.len() >= 4 && t[1] == campaign && t[2] == "store" && t[3] != "missing_config"
    })
}

/// `restore palace <campaign>`: through `mneme mp restore`, only when the store is missing.
pub struct Palace<'a> {
    pub ctx: &'a Context,
    pub campaign: String,
    name: String,
}

impl<'a> Palace<'a> {
    pub fn new(ctx: &'a Context, campaign: &str) -> Self {
        Palace {
            ctx,
            campaign: campaign.to_string(),
            name: format!("restore palace {campaign}"),
        }
    }

    fn newest_snapshot(&self, sys: &dyn System) -> Option<PathBuf> {
        let dir = self.ctx.cfg.backups.palaces.join(&self.campaign);
        sys.list_dir(&dir)
            .into_iter()
            .rev()
            .map(|n| dir.join(n))
            .find(|d| sys.path_kind(&d.join(".mneme-backup")) != PathKind::Missing)
    }
}

impl Step for Palace<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        Ok(if palace_present(self.ctx, sys, &self.campaign)? {
            Check::Done
        } else {
            Check::NeedsChange(format!("no palace store for {}", self.campaign))
        })
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        // mneme's restore replaces a present store; never let it (FR-010b).
        if palace_present(self.ctx, sys, &self.campaign)? {
            return Ok(());
        }
        let root = &self.ctx.cfg.backups.palaces;
        if !reachable(sys, root) {
            return Err(unreachable_folder(root));
        }
        let Some(snapshot) = self.newest_snapshot(sys) else {
            bail!(
                "no mneme snapshot in {}",
                root.join(&self.campaign).display()
            );
        };
        run_ok(
            sys,
            &Cmd::new(&a.bin("mneme").display().to_string())
                .args([
                    "mp",
                    "restore",
                    &self.campaign,
                    "--from",
                    &snapshot.display().to_string(),
                    "--config",
                    &a.path.display().to_string(),
                ])
                .interactive(),
        )?;
        Ok(())
    }

    fn continue_on_failure(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::fake::FakeSystem;

    #[test]
    fn newest_dated_backup_wins_and_undated_is_the_fallback() {
        let sys = FakeSystem::new("k");
        sys.set_file("/b/rpg_library.db", "u");
        assert_eq!(
            newest_backup(&sys, Path::new("/b"), "rpg_library"),
            Some(PathBuf::from("/b/rpg_library.db"))
        );
        sys.set_file("/b/rpg_library-2026-10-01.db", "a");
        sys.set_file("/b/rpg_library-2026-11-01.db", "b");
        sys.set_file("/b/user_data-2026-11-01.db", "c");
        assert_eq!(
            newest_backup(&sys, Path::new("/b"), "rpg_library"),
            Some(PathBuf::from("/b/rpg_library-2026-11-01.db"))
        );
        assert_eq!(newest_backup(&sys, Path::new("/b"), "missing"), None);
    }

    #[test]
    fn store_rows() {
        let out = "ok   obelisk              store   built                      store healthy\n";
        assert!(store_present(out, "obelisk"));
        assert!(!store_present(out, "toee"));
        let missing = "todo toee store   missing_config   not provisioned\n";
        assert!(!store_present(missing, "toee"));
    }
}
