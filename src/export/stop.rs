//! `local-machine export stop`: make the share unavailable, persistently (FR-008).

use std::path::Path;

use anyhow::Result;

use crate::system::action::{self, Action, Check, Step};
use crate::system::{Cmd, System, run_ok};

use super::config::ExportConfig;
use super::samba;
use super::state;

pub fn run(cfg: &ExportConfig, sys: &dyn System) -> Result<u8> {
    let changed = action::run(
        sys,
        Action {
            gate: Some(Box::new(state::SetUp(cfg))),
            steps: vec![Box::new(ShareRemoved), Box::new(SmbdStopped)],
            after_change: None,
        },
    )?;
    if changed {
        println!("export: not available");
    } else {
        println!("export: nothing was shared");
    }
    Ok(0)
}

struct ShareRemoved;

impl Step for ShareRemoved {
    fn name(&self) -> &str {
        "share-stanza-removed"
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        Some(format!("empty {} and reload smbd", samba::SHARE_PATH))
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        match sys.read_file(Path::new(samba::SHARE_PATH)) {
            Some(t) if t != samba::render_empty_share() => {
                Ok(Check::NeedsChange("share is defined".to_string()))
            }
            _ => Ok(Check::Done),
        }
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        sys.install_file(Path::new(samba::SHARE_PATH), &samba::render_empty_share())?;
        if state::unit_active(sys, "smbd") {
            run_ok(
                sys,
                &Cmd::new("smbcontrol")
                    .args(["smbd", "reload-config"])
                    .sudo(),
            )?;
        }
        Ok(())
    }
}

struct SmbdStopped;

impl Step for SmbdStopped {
    fn name(&self) -> &str {
        "smbd-disabled"
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        Some("disable and stop smbd".to_string())
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        if state::unit_enabled(sys, "smbd") || state::unit_active(sys, "smbd") {
            Ok(Check::NeedsChange("smbd is enabled or running".to_string()))
        } else {
            Ok(Check::Done)
        }
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        run_ok(
            sys,
            &Cmd::new("systemctl")
                .args(["disable", "--now", "smbd"])
                .sudo(),
        )?;
        Ok(())
    }
}
