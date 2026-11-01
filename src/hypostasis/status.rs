//! `local-machine hypostasis status`: every piece and every service as observed. Changes
//! nothing and never prompts (FR-019..FR-021); root-only checks report `unknown:`.

use std::path::PathBuf;

use anyhow::Result;

use crate::ExitError;
use crate::say;
use crate::system::System;
use crate::system::action::{Check, NoPrompt, Step};

use super::Context;
use super::restore;
use super::services::{self, LocalService};
use super::setup::{restore_steps, software_steps};
use super::start::show_log;

const SETUP: &str = "run 'local-machine hypostasis setup'";

/// The backup folder a restore piece reads, to word the fix when it is unmounted.
fn backup_folder(ctx: &Context, name: &str) -> Option<PathBuf> {
    let b = &ctx.cfg.backups;
    Some(match name {
        "restore rpg-library" => b.rpg_library.clone(),
        "restore flexai-social data" => b.flexai_social.clone(),
        "restore flexai-combat data" => b.flexai_combat.clone(),
        n if n.starts_with("restore palace ") => b.palaces.clone(),
        _ => return None,
    })
}

#[derive(Default)]
struct Pieces {
    lines: Vec<String>,
    missing: usize,
}

impl Pieces {
    fn report(&mut self, ctx: &Context, sys: &dyn System, step: &dyn Step) -> Result<()> {
        let name = step.name();
        match step.check(sys)? {
            Check::Done => self.lines.push(format!("ok: {name}")),
            Check::DoneWith(note) => self.lines.push(format!("ok: {name} ({note})")),
            Check::Unknown => self
                .lines
                .push(format!("unknown: {name} (run 'sudo -v' first to check)")),
            Check::NeedsChange(reason) => {
                self.missing += 1;
                let fix = match backup_folder(ctx, name) {
                    Some(dir) if !restore::reachable(sys, &dir) => {
                        format!("mount {}, then {SETUP}", dir.display())
                    }
                    _ => SETUP.to_string(),
                };
                self.lines
                    .push(format!("missing: {name} ({reason}) — {fix}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Stopped,
    Failed,
    Foreign,
    Blocked,
}

pub fn run(ctx: &Context, sys: &dyn System) -> Result<u8> {
    let look = NoPrompt(sys);
    let mut pieces = Pieces::default();
    for step in software_steps(ctx) {
        pieces.report(ctx, &look, step.as_ref())?;
    }
    let authority = ctx.authority(&look)?;
    match &authority {
        Some(a) => {
            for step in restore_steps(ctx, &look, a) {
                pieces.report(ctx, &look, step.as_ref())?;
            }
        }
        None => {
            pieces.missing += 1;
            pieces.lines.push(format!(
                "missing: restores (needs the authority file {}) — {SETUP}",
                ctx.cfg.authority.display()
            ));
        }
    }

    let svcs: Vec<LocalService> = match &authority {
        Some(a) => services::table(ctx, a),
        None => Vec::new(),
    };
    let states: Vec<State> = svcs.iter().map(|s| observe(&look, s)).collect();
    let running = states.iter().filter(|s| **s == State::Running).count();

    say!(
        "hypostasis: {}; {running} of 3 services running",
        if pieces.missing == 0 {
            "set up"
        } else {
            "not set up"
        }
    );
    for line in &pieces.lines {
        say!("{line}");
    }
    if authority.is_none() {
        say!(
            "stopped: all services (unknown until setup seeds {})",
            ctx.cfg.authority.display()
        );
    }
    for (svc, state) in svcs.iter().zip(&states) {
        let url = services::url(ctx, &look, svc.port);
        let name = svc.name;
        match state {
            State::Running => say!("running: {name:<14} {url}"),
            State::Stopped => {
                say!("stopped: {name:<14} {url} — run 'local-machine hypostasis start'");
                if services::unit_enabled(&look, svc) {
                    show_log(&look, svc);
                }
            }
            State::Failed => {
                say!(
                    "failed:  {name:<14} {url} — see the log, then run 'local-machine hypostasis start'"
                );
                show_log(&look, svc);
            }
            State::Foreign => say!(
                "foreign: {name:<14} {url} — answering but not started by local-machine; stop \
                 it, then run 'local-machine hypostasis start'"
            ),
            State::Blocked => {
                let holder = services::port_holder(&look, svc.port)
                    .unwrap_or_else(|| "another process".to_string());
                say!(
                    "blocked: {name:<14} {url} — port {} held by {holder}",
                    svc.port
                );
            }
        }
    }
    say!("see also: hypostasis status   (component pins, rendered configs, external services)");

    Ok(if pieces.missing > 0 {
        ExitError::NOT_SET_UP
    } else if running < 3 {
        ExitError::NOT_AVAILABLE
    } else {
        0
    })
}

fn observe(sys: &dyn System, svc: &LocalService) -> State {
    let active = services::unit_active(sys, svc);
    let answering = services::probe(sys, svc);
    if active && answering {
        State::Running
    } else if answering {
        State::Foreign
    } else if services::unit_failed(sys, svc) {
        State::Failed
    } else if !active && services::port_holder(sys, svc.port).is_some() {
        State::Blocked
    } else {
        State::Stopped
    }
}
