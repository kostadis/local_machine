//! Setup pieces as runner steps (data-model.md → SetupPiece). A check whose inputs do not
//! exist yet (e.g. the venv before the authority is seeded) says `NeedsChange`, not an
//! error (research R13). Pieces keep the default no-op rollback, so finished pieces survive
//! a later failure and a re-run picks up where it stopped.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::say;
use crate::system::action::{Check, Step};
use crate::system::{Cmd, PathKind, System, run_ok, runs};

use super::Context;
use super::authority::Authority;
use super::checkout;
use super::services::{self, Kind};

fn needs_authority(ctx: &Context) -> Check {
    Check::NeedsChange(format!(
        "needs the authority file {}",
        ctx.cfg.authority.display()
    ))
}

fn present(ok: bool, reason: impl FnOnce() -> String) -> Check {
    if ok {
        Check::Done
    } else {
        Check::NeedsChange(reason())
    }
}

fn path_str(p: &Path) -> String {
    p.display().to_string()
}

/// The `uv` to run: on `PATH`, else `~/.local/bin/uv` (where pipx puts it).
pub fn uv(sys: &dyn System, home: &Path) -> String {
    if runs(sys, &Cmd::new("which").args(["uv"])) {
        "uv".to_string()
    } else {
        path_str(&home.join(".local/bin/uv"))
    }
}

// ---------------------------------------------------------------------------------------
// packages / uv / linger

pub struct Packages<'a> {
    pub ctx: &'a Context,
}

/// Packages that count as present when their command resolves (a hand-installed node).
fn command_for(pkg: &str) -> Option<&'static str> {
    match pkg {
        "nodejs" => Some("node"),
        "npm" => Some("npm"),
        _ => None,
    }
}

pub fn missing_packages(sys: &dyn System, apt: &[String]) -> Vec<String> {
    apt.iter()
        .filter(|pkg| {
            if let Some(cmd) = command_for(pkg)
                && runs(sys, &Cmd::new("which").args([cmd]))
            {
                return false;
            }
            !sys.run(&Cmd::new("dpkg-query").args(["-W", "-f=${Status}", pkg.as_str()]))
                .is_ok_and(|o| o.success() && o.stdout.trim() == "install ok installed")
        })
        .cloned()
        .collect()
}

impl Step for Packages<'_> {
    fn name(&self) -> &str {
        "packages"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let missing = missing_packages(sys, &self.ctx.cfg.apt);
        Ok(present(missing.is_empty(), || {
            format!("not installed: {}", missing.join(" "))
        }))
    }

    fn notice(&self, found: &Check) -> Option<String> {
        let list = match found {
            Check::NeedsChange(reason) => reason
                .strip_prefix("not installed: ")
                .unwrap_or(reason)
                .to_string(),
            _ => self.ctx.cfg.apt.join(" "),
        };
        Some(format!("install packages: {list}"))
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let missing = missing_packages(sys, &self.ctx.cfg.apt);
        if missing.is_empty() {
            return Ok(());
        }
        run_ok(
            sys,
            &Cmd::new("env")
                .args(["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y"])
                .args(&missing)
                .sudo()
                .interactive(),
        )?;
        Ok(())
    }
}

pub struct Uv<'a> {
    pub ctx: &'a Context,
}

impl Step for Uv<'_> {
    fn name(&self) -> &str {
        "uv"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let ok = runs(sys, &Cmd::new("which").args(["uv"]))
            || sys.path_kind(&self.ctx.home().join(".local/bin/uv")) == PathKind::File;
        Ok(present(ok, || "uv is not installed".to_string()))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        run_ok(sys, &Cmd::new("pipx").args(["install", "uv"]))?;
        Ok(())
    }
}

pub struct Linger<'a> {
    pub ctx: &'a Context,
}

impl Step for Linger<'_> {
    fn name(&self) -> &str {
        "linger"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let owner = &self.ctx.machine.owner;
        let ok = sys
            .run(&Cmd::new("loginctl").args(["show-user", owner, "-p", "Linger"]))
            .is_ok_and(|o| o.success() && o.stdout.trim() == "Linger=yes");
        Ok(present(ok, || format!("linger is off for {owner}")))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        Some(format!("enable linger for {}", self.ctx.machine.owner))
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        run_ok(
            sys,
            &Cmd::new("loginctl")
                .args(["enable-linger", &self.ctx.machine.owner])
                .sudo(),
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// authority / component-paths

pub struct AuthorityFile<'a> {
    pub ctx: &'a Context,
}

impl Step for AuthorityFile<'_> {
    fn name(&self) -> &str {
        "authority"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let path = &self.ctx.cfg.authority;
        Ok(present(sys.path_kind(path) != PathKind::Missing, || {
            format!("{} does not exist", path.display())
        }))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let target = &self.ctx.cfg.authority;
        if sys.path_kind(target) != PathKind::Missing {
            return Ok(());
        }
        let source = &self.ctx.cfg.authority_source;
        if sys.path_kind(source) != PathKind::File || sys.read_file(source).is_none() {
            bail!(
                "authority_source {} is not readable (it should come with a checkout)",
                source.display()
            );
        }
        if let Some(parent) = target.parent()
            && sys.path_kind(parent) == PathKind::Missing
        {
            sys.make_dir_all(parent)?;
        }
        sys.copy_file(source, target)
    }
}

pub struct ComponentPaths<'a> {
    pub ctx: &'a Context,
}

impl ComponentPaths<'_> {
    fn uncovered(&self, authority: &Authority) -> Vec<PathBuf> {
        authority
            .component_paths
            .iter()
            .filter(|p| !checkout::covers(&self.ctx.cfg.checkouts, p))
            .cloned()
            .collect()
    }
}

fn joined(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl Step for ComponentPaths<'_> {
    fn name(&self) -> &str {
        "component-paths"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let Some(a) = self.ctx.authority(sys)? else {
            return Ok(needs_authority(self.ctx));
        };
        let uncovered = self.uncovered(&a);
        Ok(present(uncovered.is_empty(), || {
            format!("not covered by a checkout: {}", joined(&uncovered))
        }))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    /// Check-only: never changes anything.
    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        let uncovered = self.uncovered(&a);
        if !uncovered.is_empty() {
            bail!(
                "component paths in {} not covered by a checkout in {}: {}",
                a.path.display(),
                self.ctx.cfg.file.display(),
                joined(&uncovered)
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// venv / hypostasis / components

pub struct Venv<'a> {
    pub ctx: &'a Context,
}

impl Step for Venv<'_> {
    fn name(&self) -> &str {
        "venv"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let Some(a) = self.ctx.authority(sys)? else {
            return Ok(needs_authority(self.ctx));
        };
        let ok = runs(sys, &Cmd::new(&path_str(&a.python())).args(["--version"]));
        Ok(present(ok, || {
            format!("{} does not exist", a.venv.display())
        }))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        run_ok(
            sys,
            &Cmd::new(&uv(sys, self.ctx.home())).args(["venv", &path_str(&a.venv)]),
        )?;
        Ok(())
    }
}

pub struct Hypostasis<'a> {
    pub ctx: &'a Context,
}

impl Step for Hypostasis<'_> {
    fn name(&self) -> &str {
        "hypostasis"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let Some(a) = self.ctx.authority(sys)? else {
            return Ok(needs_authority(self.ctx));
        };
        let ok = runs(
            sys,
            &Cmd::new(&path_str(&a.bin("hypostasis"))).args(["--help"]),
        );
        Ok(present(ok, || {
            format!("hypostasis is not installed in {}", a.venv.display())
        }))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        run_ok(
            sys,
            &Cmd::new(&uv(sys, self.ctx.home())).args([
                "pip",
                "install",
                "--python",
                &path_str(&a.python()),
                "-e",
                &path_str(&self.ctx.cfg.hypostasis_source),
            ]),
        )?;
        Ok(())
    }
}

pub struct Components<'a> {
    pub ctx: &'a Context,
}

impl Step for Components<'_> {
    fn name(&self) -> &str {
        "components"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let Some(a) = self.ctx.authority(sys)? else {
            return Ok(needs_authority(self.ctx));
        };
        if a.component_names.is_empty() {
            return Ok(Check::Done);
        }
        let ok = runs(
            sys,
            &Cmd::new(&uv(sys, self.ctx.home()))
                .args(["pip", "show", "--python", &path_str(&a.python())])
                .args(&a.component_names),
        );
        Ok(present(ok, || {
            format!("not all of {} installed", a.component_names.join(", "))
        }))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        let cmd = Cmd::new(&path_str(&a.bin("hypostasis"))).args([
            "install",
            "--config",
            &path_str(&a.path),
        ]);
        let out = sys.run(&cmd)?;
        for line in out.stdout.lines() {
            say!("  | {line}");
        }
        if !out.success() {
            let detail = out.stderr.trim();
            bail!("`{}` exited with {}:\n{detail}", cmd.line(), out.code);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// services-venv / install <service> / unit <service>

pub struct ServicesVenv<'a> {
    pub ctx: &'a Context,
}

impl Step for ServicesVenv<'_> {
    fn name(&self) -> &str {
        "services-venv"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let py = services::services_python(self.ctx);
        let ok = runs(sys, &Cmd::new(&path_str(&py)).args(["--version"]));
        Ok(present(ok, || {
            format!("{} does not exist", self.ctx.cfg.services_venv.display())
        }))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        run_ok(
            sys,
            &Cmd::new(&uv(sys, self.ctx.home()))
                .args(["venv", &path_str(&self.ctx.cfg.services_venv)]),
        )?;
        Ok(())
    }
}

pub struct Install<'a> {
    pub ctx: &'a Context,
    pub kind: Kind,
    name: String,
}

impl<'a> Install<'a> {
    pub fn new(ctx: &'a Context, kind: Kind) -> Self {
        Install {
            ctx,
            kind,
            name: format!("install {}", kind.name()),
        }
    }

    fn dist_index(&self) -> PathBuf {
        services::app_dir(self.ctx, Kind::RpgLib).join("frontend/dist/index.html")
    }

    fn installed(&self, sys: &dyn System) -> bool {
        let py = path_str(&services::services_python(self.ctx));
        match self.kind {
            Kind::RpgLib => {
                runs(
                    sys,
                    &Cmd::new(&uv(sys, self.ctx.home())).args([
                        "pip",
                        "show",
                        "--python",
                        &py,
                        "rpg-lib",
                        "claudelib",
                    ]),
                ) && sys.path_kind(&self.dist_index()) == PathKind::File
            }
            Kind::FlexSocial | Kind::FlexCombat => {
                runs(sys, &Cmd::new(&py).args(["-c", "import flask, openpyxl"]))
            }
        }
    }
}

impl Step for Install<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        Ok(present(self.installed(sys), || {
            format!(
                "{} is not installed in {}",
                self.kind.name(),
                self.ctx.cfg.services_venv.display()
            )
        }))
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let uv = uv(sys, self.ctx.home());
        let py = path_str(&services::services_python(self.ctx));
        let mytools = &self.ctx.cfg.mytools;
        match self.kind {
            Kind::RpgLib => {
                run_ok(
                    sys,
                    &Cmd::new(&uv).args([
                        "pip",
                        "install",
                        "--python",
                        &py,
                        "-e",
                        &path_str(&mytools.join("claudelib")),
                        "-e",
                        &path_str(&mytools.join("rpg-lib")),
                    ]),
                )?;
                if sys.path_kind(&self.dist_index()) != PathKind::File {
                    let frontend =
                        path_str(&services::app_dir(self.ctx, Kind::RpgLib).join("frontend"));
                    run_ok(sys, &Cmd::new("npm").args(["--prefix", &frontend, "ci"]))?;
                    run_ok(
                        sys,
                        &Cmd::new("npm").args(["--prefix", &frontend, "run", "build"]),
                    )?;
                }
            }
            Kind::FlexSocial | Kind::FlexCombat => {
                let req = services::app_dir(self.ctx, self.kind).join("requirements.txt");
                run_ok(
                    sys,
                    &Cmd::new(&uv).args(["pip", "install", "--python", &py, "-r", &path_str(&req)]),
                )?;
            }
        }
        Ok(())
    }
}

pub struct Unit<'a> {
    pub ctx: &'a Context,
    pub kind: Kind,
    name: String,
}

impl<'a> Unit<'a> {
    pub fn new(ctx: &'a Context, kind: Kind) -> Self {
        Unit {
            ctx,
            kind,
            name: format!("unit {}", kind.name()),
        }
    }
}

impl Step for Unit<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let Some(a) = self.ctx.authority(sys)? else {
            return Ok(needs_authority(self.ctx));
        };
        let svc = services::service(self.ctx, self.kind, &a);
        let wanted = services::render_unit(self.ctx, &svc, &a);
        Ok(match sys.read_file(&svc.unit_path) {
            None => Check::NeedsChange(format!("{} does not exist", svc.unit_path.display())),
            Some(text) if text == wanted => Check::Done,
            Some(_) => Check::NeedsChange("unit changed".to_string()),
        })
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    /// Writes the unit and reloads systemd; never starts, stops or restarts it (FR-009).
    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        let svc = services::service(self.ctx, self.kind, &a);
        let existed = sys.read_file(&svc.unit_path).is_some();
        for dir in [
            services::unit_dir(self.ctx.home()),
            services::log_dir(self.ctx.home()),
        ] {
            if sys.path_kind(&dir) == PathKind::Missing {
                sys.make_dir_all(&dir)?;
            }
        }
        sys.write_user_file(&svc.unit_path, &services::render_unit(self.ctx, &svc, &a))?;
        run_ok(
            sys,
            &Cmd::new("systemctl").args(["--user", "daemon-reload"]),
        )?;
        if existed && services::unit_active(sys, &svc) {
            say!("note: {}: unit changed; run stop then start", svc.name);
        }
        Ok(())
    }
}
