//! `local-machine hypostasis stop`: take the services down and keep them down (FR-026).
//! Never needs root, never prompts, and never stops what it did not start (FR-028).

use std::time::Duration;

use anyhow::Result;

use crate::ExitError;
use crate::say;
use crate::system::System;

use super::Context;
use super::services::{self, LocalService};
use super::start::{require_authority, systemctl};

const POLL: Duration = Duration::from_millis(250);
const STOP_WAIT: u32 = 60; // × 250 ms = 15 s

pub fn run(ctx: &Context, sys: &dyn System) -> Result<u8> {
    let authority = require_authority(ctx, sys)?;
    let mut failed = 0;
    for svc in services::table(ctx, &authority) {
        if !stop_one(sys, &svc) {
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(ExitError::new(
            ExitError::FAILED,
            format!("stop: {failed} of 3 services could not be stopped"),
        )
        .into());
    }
    Ok(0)
}

fn stop_one(sys: &dyn System, svc: &LocalService) -> bool {
    let active = services::unit_active(sys, svc);
    if !active {
        if services::probe(sys, svc) {
            say!(
                "error: {}: answering but not started by local-machine; not stopping it",
                svc.name
            );
            return false;
        }
        // Inactive but still enabled would come back after a reboot.
        if services::unit_enabled(sys, svc)
            && let Err(e) = systemctl(sys, &["disable"], svc)
        {
            say!("error: {}: {e:#}", svc.name);
            return false;
        }
        say!("ok: {} (stopped)", svc.name);
        return true;
    }
    if let Err(e) = systemctl(sys, &["disable", "--now"], svc) {
        say!("error: {}: {e:#}", svc.name);
        return false;
    }
    for _ in 0..STOP_WAIT {
        if !services::probe(sys, svc) {
            say!("changed: {} (stopped)", svc.name);
            return true;
        }
        sys.sleep(POLL);
    }
    say!(
        "error: {}: still answering 15 s after its unit stopped",
        svc.name
    );
    false
}
