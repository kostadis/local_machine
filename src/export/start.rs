//! `local-machine export start`: make the share available, persistently (FR-006).

use std::path::Path;

use anyhow::Result;

use crate::system::action::{self, Action, Check, Step};
use crate::system::files::{self, Backup};
use crate::system::{Cmd, System, run_ok};

use super::config::ExportConfig;
use super::setup::{reload_smbd, validate};
use super::state;
use super::{connect, samba};

pub fn run(cfg: &ExportConfig, sys: &dyn System) -> Result<u8> {
    // A running smbd only sees a new share after a reload; a freshly started one already has it.
    let after_change =
        state::unit_active(sys, "smbd").then(|| reload_smbd("reload smbd, if the share changes"));
    action::run(
        sys,
        Action {
            gate: Some(Box::new(state::SetUp(cfg))),
            steps: vec![
                Box::new(ShareStanza {
                    cfg: cfg.clone(),
                    backup: None,
                }),
                Box::new(SmbdRunning),
            ],
            after_change,
        },
    )?;
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

    fn notice(&self, _found: &Check) -> Option<String> {
        Some(format!("write {}", samba::SHARE_PATH))
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

    fn notice(&self, _found: &Check) -> Option<String> {
        Some("enable and start smbd".to_string())
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
