//! `local-machine export start`: make the share available, persistently (FR-006).

use std::path::Path;

use anyhow::{Result, bail};

use crate::ExitError;
use crate::system::files::{self, Backup};
use crate::system::steps::{Check, Step, run_steps};
use crate::system::{Cmd, System, run_ok, sudo_preflight};

use super::config::ExportConfig;
use super::setup::validate;
use super::state::{self, ShareState};
use super::{connect, samba};

pub fn run(cfg: &ExportConfig, sys: &dyn System) -> Result<u8> {
    sudo_preflight(
        sys,
        &[
            format!("write {}", samba::SHARE_PATH),
            "enable and start smbd".to_string(),
            "reload smbd".to_string(),
        ],
    )?;
    let report = state::detect(cfg, sys);
    if report.state == ShareState::NotSetUp {
        return Err(ExitError::new(ExitError::NOT_SET_UP, report.not_set_up_message()).into());
    }
    if !report.unknown.is_empty() {
        bail!(
            "could not check {}: sudo refused",
            report.unknown.join(", ")
        );
    }

    let share_changes = sys.read_file(Path::new(samba::SHARE_PATH)).as_deref()
        != Some(samba::render_share(cfg).as_str());
    let mut steps: Vec<Box<dyn Step>> = vec![
        Box::new(ShareStanza {
            cfg: cfg.clone(),
            backup: None,
        }),
        Box::new(SmbdRunning),
    ];
    run_steps(sys, &mut steps)?;
    // A running smbd only sees the new share after a reload; a freshly started one already has it.
    if share_changes && report.smbd_active {
        run_ok(
            sys,
            &Cmd::new("smbcontrol")
                .args(["smbd", "reload-config"])
                .sudo(),
        )?;
        println!("changed: reload smbd");
    }

    println!("export: available");
    for line in connect::lines(cfg, connect::local_address(cfg, sys)) {
        println!("{line}");
    }
    Ok(0)
}

struct ShareStanza {
    cfg: ExportConfig,
    backup: Option<Backup>,
}

impl Step for ShareStanza {
    fn name(&self) -> &str {
        "share-stanza"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let current = sys.read_file(Path::new(samba::SHARE_PATH));
        if current.as_deref() == Some(samba::render_share(&self.cfg).as_str()) {
            Ok(Check::Done)
        } else {
            Ok(Check::NeedsChange("share definition differs".to_string()))
        }
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let backup = files::replace(
            sys,
            Path::new(samba::SHARE_PATH),
            &samba::render_share(&self.cfg),
        )?;
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

struct SmbdRunning;

impl Step for SmbdRunning {
    fn name(&self) -> &str {
        "smbd-enabled"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        if state::unit_enabled(sys, "smbd") && state::unit_active(sys, "smbd") {
            Ok(Check::Done)
        } else {
            Ok(Check::NeedsChange(
                "smbd is not enabled and running".to_string(),
            ))
        }
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        run_ok(
            sys,
            &Cmd::new("systemctl")
                .args(["enable", "--now", "smbd"])
                .sudo(),
        )?;
        Ok(())
    }
}
