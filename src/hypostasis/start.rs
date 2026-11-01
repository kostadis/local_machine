//! `local-machine hypostasis start`: bring up the three services as user units and report
//! each once it answers (FR-023..FR-029a). Never needs root, never prompts.

use std::time::Duration;

use anyhow::Result;

use crate::ExitError;
use crate::say;
use crate::system::action::{Check, NoPrompt, Step};
use crate::system::{Cmd, System};

use super::Context;
use super::authority::Authority;
use super::pieces::{Install, Unit};
use super::restore::{FlexData, RpgLibrary};
use super::services::{self, Kind, LocalService};

const POLL: Duration = Duration::from_millis(250);
const START_WAIT: u32 = 120; // × 250 ms = 30 s
pub const LOG_LINES: usize = 20;

/// Why a service is not ready to start (FR-023), or `None` when it is.
pub fn not_ready(ctx: &Context, sys: &dyn System, kind: Kind) -> Result<Option<String>> {
    let look = NoPrompt(sys);
    let data: Box<dyn Step> = match kind {
        Kind::RpgLib => Box::new(RpgLibrary { ctx }),
        _ => Box::new(FlexData::new(ctx, kind)),
    };
    let pieces: Vec<Box<dyn Step>> = vec![
        Box::new(Install::new(ctx, kind)),
        Box::new(Unit::new(ctx, kind)),
        data,
    ];
    for piece in pieces {
        match piece.check(&look)? {
            Check::Done | Check::DoneWith(_) => {}
            Check::NeedsChange(reason) => {
                return Ok(Some(format!("{}: {reason}", piece.name())));
            }
            Check::Unknown => return Ok(Some(format!("{}: cannot check", piece.name()))),
        }
    }
    Ok(None)
}

fn not_set_up(lines: Vec<String>) -> anyhow::Error {
    let mut msg = "not set up — run 'local-machine hypostasis setup'".to_string();
    for l in lines {
        msg.push_str(&format!("\n  not ready: {l}"));
    }
    ExitError::new(ExitError::NOT_SET_UP, msg).into()
}

/// The authority, or exit 4 when setup has not seeded it yet.
pub fn require_authority(ctx: &Context, sys: &dyn System) -> Result<Authority> {
    ctx.authority(sys)?.ok_or_else(|| {
        not_set_up(vec![format!(
            "all services ({} does not exist)",
            ctx.cfg.authority.display()
        )])
    })
}

pub fn systemctl(sys: &dyn System, verb: &[&str], svc: &LocalService) -> Result<()> {
    crate::system::run_ok(
        sys,
        &Cmd::new("systemctl")
            .args(["--user"])
            .args(verb)
            .args([svc.unit_name.as_str()]),
    )?;
    Ok(())
}

/// The last log lines, indented, then the log path (FR-029a).
pub fn show_log(sys: &dyn System, svc: &LocalService) {
    for line in services::log_tail(sys, &svc.log_path, LOG_LINES) {
        say!("  | {line}");
    }
    say!("  log: {}", svc.log_path.display());
}

pub fn run(ctx: &Context, sys: &dyn System) -> Result<u8> {
    let authority = require_authority(ctx, sys)?;
    let mut problems = Vec::new();
    for kind in Kind::ALL {
        if let Some(reason) = not_ready(ctx, sys, kind)? {
            problems.push(format!("{} ({reason})", kind.name()));
        }
    }
    if !problems.is_empty() {
        return Err(not_set_up(problems));
    }

    let mut failed = 0;
    for svc in services::table(ctx, &authority) {
        if !start_one(ctx, sys, &svc) {
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(ExitError::new(
            ExitError::FAILED,
            format!("start: {failed} of 3 services did not start"),
        )
        .into());
    }
    Ok(0)
}

/// Start one service; prints its line. Returns whether it is running.
fn start_one(ctx: &Context, sys: &dyn System, svc: &LocalService) -> bool {
    let url = services::url(ctx, sys, svc.port);
    let active = services::unit_active(sys, svc);
    let answering = services::probe(sys, svc);
    if active && answering {
        say!("ok: {} (running at {url})", svc.name);
        return true;
    }
    if answering {
        say!(
            "error: {}: answering at {url} but not started by local-machine; not starting a \
             second copy",
            svc.name
        );
        return false;
    }
    if !active && let Some(holder) = services::port_holder(sys, svc.port) {
        say!("error: {}: port {} held by {holder}", svc.name, svc.port);
        return false;
    }
    say!("adding: {}", svc.name);
    if let Err(e) = systemctl(sys, &["enable", "--now"], svc) {
        say!("error: {}: {e:#}", svc.name);
        show_log(sys, svc);
        return false;
    }
    for _ in 0..START_WAIT {
        if services::probe(sys, svc) {
            say!("changed: {} (running at {url})", svc.name);
            return true;
        }
        sys.sleep(POLL);
    }
    let cause = if services::unit_failed(sys, svc) {
        "the unit failed".to_string()
    } else {
        "did not answer within 30 s".to_string()
    };
    // Nothing half-started may claim to be up (US3-7).
    let _ = systemctl(sys, &["disable", "--now"], svc);
    say!("error: {}: {cause}", svc.name);
    show_log(sys, svc);
    false
}
