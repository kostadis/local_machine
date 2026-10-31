//! `local-machine export stop`: make the share unavailable, persistently (FR-008).

use std::path::Path;

use anyhow::{Result, bail};

use crate::ExitError;
use crate::system::steps::{Check, Step, run_steps};
use crate::system::{Cmd, System, run_ok, sudo_preflight};

use super::config::ExportConfig;
use super::samba;
use super::state::{self, ShareState};

pub fn run(cfg: &ExportConfig, sys: &dyn System) -> Result<u8> {
    // Refuse before prompting when the unprivileged checks already show setup is missing.
    let report = state::detect(cfg, sys);
    if report.state == ShareState::NotSetUp {
        return Err(ExitError::new(ExitError::NOT_SET_UP, report.not_set_up_message()).into());
    }
    sudo_preflight(
        sys,
        &[
            format!("empty {}", samba::SHARE_PATH),
            "reload smbd".to_string(),
            "disable and stop smbd".to_string(),
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
    if !report.share_defined && !report.smbd_enabled && !report.smbd_active {
        println!("export: nothing was shared");
        return Ok(0);
    }

    let mut steps: Vec<Box<dyn Step>> = vec![Box::new(ShareRemoved), Box::new(SmbdStopped)];
    run_steps(sys, &mut steps)?;
    println!("export: not available");
    Ok(0)
}

struct ShareRemoved;

impl Step for ShareRemoved {
    fn name(&self) -> &str {
        "share-stanza-removed"
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
