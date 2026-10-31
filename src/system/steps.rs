//! Idempotent actions as a list of check-then-apply steps, with rollback on failure.
//!
//! A step whose own `apply` fails partway undoes its partial work before returning the
//! error; `run_steps` then rolls back the steps that completed before it.

use anyhow::{Result, anyhow};

use super::System;

pub enum Check {
    Done,
    /// Already correct, with a note for the owner.
    DoneWith(String),
    NeedsChange(String),
}

pub trait Step {
    fn name(&self) -> &str;
    fn check(&self, sys: &dyn System) -> Result<Check>;
    fn apply(&mut self, sys: &dyn System) -> Result<()>;
    fn rollback(&mut self, _sys: &dyn System) -> Result<()> {
        Ok(())
    }
}

/// Check and apply each step in order, printing `ok:`/`changed:`. On the first failure,
/// roll back the steps already applied (newest first) and return the error.
///
/// Returns whether anything changed. Callers run [`super::sudo_preflight`] beforehand.
pub fn run_steps(sys: &dyn System, steps: &mut [Box<dyn Step>]) -> Result<bool> {
    let mut applied: Vec<usize> = Vec::new();
    for i in 0..steps.len() {
        let result = match steps[i].check(sys) {
            Ok(Check::Done) => {
                println!("ok: {}", steps[i].name());
                continue;
            }
            Ok(Check::DoneWith(note)) => {
                println!("ok: {} ({note})", steps[i].name());
                continue;
            }
            Ok(Check::NeedsChange(_)) => steps[i].apply(sys),
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => {
                println!("changed: {}", steps[i].name());
                applied.push(i);
            }
            Err(e) => {
                for &j in applied.iter().rev() {
                    if let Err(re) = steps[j].rollback(sys) {
                        eprintln!("error: rolling back {}: {re:#}", steps[j].name());
                    }
                }
                return Err(anyhow!("{}: {e:#}", steps[i].name()));
            }
        }
    }
    Ok(!applied.is_empty())
}
