//! `local-machine hypostasis setup`: put every missing piece in place, in data-model order
//! (FR-005..FR-017). Root work (packages, linger, firewall) is asked for once, up front,
//! through the shared runner; everything else runs as the owner. Starts nothing (FR-009).

use anyhow::{Result, anyhow};

use crate::say;
use crate::system::System;
use crate::system::action::{self, Action, Step, StepFailure};

use super::Context;
use super::authority::Authority;
use super::checkout::CheckoutStep;
use super::firewall::FirewallStep;
use super::pieces::{
    AuthorityFile, ComponentPaths, Components, Hypostasis, Install, Linger, Packages, ServicesVenv,
    Unit, Uv, Venv,
};
use super::restore::{FlexData, Palace, RpgLibrary};
use super::services::Kind;

/// Every software piece, in order. Root pieces come before the slow ones (analysis I1).
pub fn software_steps(ctx: &Context) -> Vec<Box<dyn Step + '_>> {
    let mut steps: Vec<Box<dyn Step + '_>> = vec![
        Box::new(Packages { ctx }),
        Box::new(Uv { ctx }),
        Box::new(Linger { ctx }),
    ];
    for co in &ctx.cfg.checkouts {
        steps.push(Box::new(CheckoutStep::new(co)));
    }
    steps.push(Box::new(AuthorityFile { ctx }));
    for kind in Kind::ALL {
        steps.push(Box::new(FirewallStep::new(ctx, kind)));
    }
    steps.push(Box::new(ComponentPaths { ctx }));
    steps.push(Box::new(Venv { ctx }));
    steps.push(Box::new(Hypostasis { ctx }));
    steps.push(Box::new(Components { ctx }));
    steps.push(Box::new(ServicesVenv { ctx }));
    for kind in Kind::ALL {
        steps.push(Box::new(Install::new(ctx, kind)));
    }
    for kind in Kind::ALL {
        steps.push(Box::new(Unit::new(ctx, kind)));
    }
    steps
}

/// Every restore, last (FR-010d). Palaces need the authority's campaign list.
pub fn restore_steps<'a>(
    ctx: &'a Context,
    sys: &dyn System,
    authority: &Authority,
) -> Vec<Box<dyn Step + 'a>> {
    let mut steps: Vec<Box<dyn Step + 'a>> = vec![
        Box::new(RpgLibrary { ctx }),
        Box::new(FlexData::new(ctx, Kind::FlexSocial)),
        Box::new(FlexData::new(ctx, Kind::FlexCombat)),
    ];
    for campaign in authority.campaign_names(sys) {
        steps.push(Box::new(Palace::new(ctx, &campaign)));
    }
    steps
}

pub fn run(ctx: &Context, sys: &dyn System) -> Result<u8> {
    let software = action::run(
        sys,
        Action {
            gate: None,
            steps: software_steps(ctx),
            after_change: None,
        },
    );
    if let Err(e) = software {
        if let Some(f) = e.downcast_ref::<StepFailure>() {
            say!("done: {} pieces", f.done.len());
            say!("remaining: {}", f.remaining.join(", "));
        }
        return Err(e);
    }

    // The authority exists now: the software pieces above needed it.
    let authority = ctx
        .authority(sys)?
        .ok_or_else(|| anyhow!("{} does not exist", ctx.cfg.authority.display()))?;
    action::run(
        sys,
        Action {
            gate: None,
            steps: restore_steps(ctx, sys, &authority),
            after_change: None,
        },
    )?;

    say!("next: local-machine hypostasis start");
    say!("then: mneme up <campaign>");
    Ok(0)
}
