//! `local-machine export setup`: install and configure everything sharing needs (FR-004,
//! FR-005, FR-015, FR-016). Never changes whether the share is available.

use std::path::Path;

use anyhow::{Result, bail};

use crate::system::files::{self, Backup};
use crate::system::steps::{Check, Step, run_steps};
use crate::system::{Cmd, System, run_ok, sudo_preflight};

use super::config::ExportConfig;
use super::samba;
use super::state::{self, Presence};

pub fn run(cfg: &ExportConfig, sys: &dyn System) -> Result<u8> {
    sudo_preflight(
        sys,
        &[
            "install samba (apt-get)".to_string(),
            format!(
                "write {} and include it from {}",
                samba::GLOBALS_PATH,
                samba::SMB_CONF
            ),
            format!("set the Samba password for {} (smbpasswd)", cfg.owner),
            format!(
                "allow Samba from {} in ufw, if ufw is active",
                cfg.allowed_network
            ),
            "disable nmbd".to_string(),
        ],
    )?;
    let smbd_active_before = state::unit_active(sys, "smbd");
    let mut steps: Vec<Box<dyn Step>> = vec![
        Box::new(SambaPackage),
        Box::new(GlobalConfig::new(cfg)),
        Box::new(IncludeLine::default()),
        Box::new(SambaUser { cfg: cfg.clone() }),
        Box::new(Firewall { cfg: cfg.clone() }),
        Box::new(NmbdDisabled),
    ];
    let changed = run_steps(sys, &mut steps)?;
    // New global settings (e.g. hosts allow) take effect on a running share without
    // changing whether it is available.
    if changed && smbd_active_before {
        run_ok(
            sys,
            &Cmd::new("smbcontrol")
                .args(["smbd", "reload-config"])
                .sudo(),
        )?;
        println!("changed: reload smbd");
    }
    println!("export: set up");
    Ok(0)
}

/// A step's check, from the shared component check. `Unknown` after the sudo preflight
/// means sudo still refused, which is an error rather than a guess.
fn component_check(name: &str, cfg: &ExportConfig, sys: &dyn System) -> Result<Check> {
    to_check(name, state::check(name, cfg, sys))
}

fn to_check(name: &str, presence: Presence) -> Result<Check> {
    match presence {
        Presence::Present => Ok(Check::Done),
        Presence::PresentWith(note) => Ok(Check::DoneWith(note)),
        Presence::Missing { reason, .. } => Ok(Check::NeedsChange(reason)),
        Presence::Unknown => bail!("could not check {name}: sudo refused"),
    }
}

/// `testparm` must accept the combined configuration before a change is kept.
pub fn validate(sys: &dyn System) -> Result<()> {
    let out = sys.run(&Cmd::new("testparm").args(["-s"]))?;
    if !out.success() {
        bail!("testparm rejected the configuration: {}", out.stderr.trim());
    }
    Ok(())
}

struct SambaPackage;

impl Step for SambaPackage {
    fn name(&self) -> &str {
        state::SAMBA_PACKAGE
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        to_check(state::SAMBA_PACKAGE, state::samba_package(sys))
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        // The package enables and starts smbd; put it back the way it was (FR-004).
        let was_enabled = state::unit_enabled(sys, "smbd");
        let was_active = state::unit_active(sys, "smbd");
        run_ok(
            sys,
            &Cmd::new("env")
                .args([
                    "DEBIAN_FRONTEND=noninteractive",
                    "apt-get",
                    "install",
                    "-y",
                    "samba",
                ])
                .sudo()
                .interactive(),
        )?;
        if !was_enabled {
            run_ok(
                sys,
                &Cmd::new("systemctl")
                    .args(["disable", "--now", "smbd"])
                    .sudo(),
            )?;
        } else if !was_active {
            run_ok(sys, &Cmd::new("systemctl").args(["stop", "smbd"]).sudo())?;
        }
        Ok(())
    }
}

struct GlobalConfig {
    cfg: ExportConfig,
    backup: Option<Backup>,
    created_share_file: bool,
}

impl GlobalConfig {
    fn new(cfg: &ExportConfig) -> Self {
        GlobalConfig {
            cfg: cfg.clone(),
            backup: None,
            created_share_file: false,
        }
    }

    fn undo(&mut self, sys: &dyn System) -> Result<()> {
        if let Some(b) = self.backup.take() {
            b.restore(sys)?;
        }
        if self.created_share_file {
            sys.remove_file(Path::new(samba::SHARE_PATH))?;
            self.created_share_file = false;
        }
        Ok(())
    }
}

impl Step for GlobalConfig {
    fn name(&self) -> &str {
        state::GLOBAL_CONFIG
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        component_check(state::GLOBAL_CONFIG, &self.cfg, sys)
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        self.backup = Some(files::replace(
            sys,
            Path::new(samba::GLOBALS_PATH),
            &samba::render_globals(&self.cfg),
        )?);
        if sys.read_file(Path::new(samba::SHARE_PATH)).is_none() {
            sys.install_file(Path::new(samba::SHARE_PATH), &samba::render_empty_share())?;
            self.created_share_file = true;
        }
        // Only meaningful once smb.conf includes the file (first run: next step validates).
        let included = sys
            .read_file(Path::new(samba::SMB_CONF))
            .is_some_and(|t| state::include_count(&t) > 0);
        if included && let Err(e) = validate(sys) {
            self.undo(sys)?;
            return Err(e);
        }
        Ok(())
    }

    fn rollback(&mut self, sys: &dyn System) -> Result<()> {
        self.undo(sys)
    }
}

#[derive(Default)]
struct IncludeLine {
    backup: Option<Backup>,
}

impl Step for IncludeLine {
    fn name(&self) -> &str {
        state::INCLUDE_LINE
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        to_check(state::INCLUDE_LINE, state::include_line(sys))
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let path = Path::new(samba::SMB_CONF);
        let Some(text) = sys.read_file(path) else {
            bail!("{} does not exist", samba::SMB_CONF);
        };
        let wanted = samba::include_line();
        let mut lines: Vec<&str> = text.lines().filter(|l| l.trim() != wanted).collect();
        lines.push(&wanted);
        let mut new_text = lines.join("\n");
        new_text.push('\n');
        let backup = files::replace(sys, path, &new_text)?;
        if let Err(e) = validate(sys) {
            backup.restore(sys)?;
            return Err(e);
        }
        self.backup = Some(backup);
        Ok(())
    }

    fn rollback(&mut self, sys: &dyn System) -> Result<()> {
        match self.backup.take() {
            Some(b) => b.restore(sys),
            None => Ok(()),
        }
    }
}

struct SambaUser {
    cfg: ExportConfig,
}

impl Step for SambaUser {
    fn name(&self) -> &str {
        state::SAMBA_USER
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        component_check(state::SAMBA_USER, &self.cfg, sys)
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let owner = &self.cfg.owner;
        let first = sys.prompt_password(&format!("share password for {owner}: "))?;
        if first.is_empty() {
            bail!("the share password must not be empty");
        }
        let second = sys.prompt_password("again: ")?;
        if first != second {
            bail!("the passwords did not match");
        }
        // The password goes to smbpasswd on stdin only; it is never printed or stored (FR-015).
        run_ok(
            sys,
            &Cmd::new("smbpasswd")
                .args(["-s", "-a", owner])
                .sudo()
                .stdin(format!("{first}\n{first}\n")),
        )?;
        Ok(())
    }
}

struct Firewall {
    cfg: ExportConfig,
}

impl Step for Firewall {
    fn name(&self) -> &str {
        state::FIREWALL
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        component_check(state::FIREWALL, &self.cfg, sys)
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let out = run_ok(sys, &Cmd::new("ufw").args(["status"]).sudo())?;
        let Some(sources) = state::parse_ufw(&out.stdout) else {
            return Ok(());
        };
        let wanted = self.cfg.allowed_network.to_string();
        // A rule for a previous network must go, so only the configured network gets in.
        for old in sources.iter().filter(|s| **s != wanted) {
            run_ok(
                sys,
                &Cmd::new("ufw")
                    .args(["delete", "allow", "from", old, "to", "any", "app", "Samba"])
                    .sudo(),
            )?;
        }
        if !sources.contains(&wanted) {
            run_ok(
                sys,
                &Cmd::new("ufw")
                    .args(["allow", "from", &wanted, "to", "any", "app", "Samba"])
                    .sudo(),
            )?;
        }
        Ok(())
    }
}

struct NmbdDisabled;

impl Step for NmbdDisabled {
    fn name(&self) -> &str {
        state::NMBD_DISABLED
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        to_check(state::NMBD_DISABLED, state::nmbd_disabled(sys))
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        run_ok(
            sys,
            &Cmd::new("systemctl")
                .args(["disable", "--now", "nmbd"])
                .sudo(),
        )?;
        Ok(())
    }
}
