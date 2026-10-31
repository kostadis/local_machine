//! Privileged actions, shared by every subcommand: look without prompting, then act.
//!
//! An action is an optional [`Gate`] (what must already be true) and a list of [`Step`]s
//! (what to make true). [`run`] checks everything first through [`NoPrompt`], so checks can
//! only use `sudo -n`. It prompts for sudo only when a step needs root to change something
//! or to find out, and its privilege notice names only those steps. When everything is
//! already in place it prints `ok:` lines and never prompts.

use anyhow::{Result, anyhow, bail};

use super::{Cmd, Output, Root, System, sudo_preflight};
use crate::ExitError;

pub enum Check {
    Done,
    /// Already correct, with a note for the owner.
    DoneWith(String),
    NeedsChange(String),
    /// Only root can tell (`sudo -n` refused).
    Unknown,
}

pub trait Step {
    fn name(&self) -> &str;
    /// Must not prompt or change anything; it is given a [`NoPrompt`] system.
    fn check(&self, sys: &dyn System) -> Result<Check>;
    /// The privilege-notice line for this step given what `check` found
    /// (`NeedsChange` or `Unknown`), or `None` when it needs no root.
    fn notice(&self, found: &Check) -> Option<String>;
    /// A step whose `apply` fails partway undoes its partial work before returning.
    fn apply(&mut self, sys: &dyn System) -> Result<()>;
    fn rollback(&mut self, _sys: &dyn System) -> Result<()> {
        Ok(())
    }
}

pub enum GateResult {
    Pass,
    /// The action must not run; ends with this error's exit code, before any prompt.
    Refuse(ExitError),
    /// Only root can tell; the notice lines for finding out.
    Unknown(Vec<String>),
}

/// A condition that must already hold before an action may run (e.g. "set up").
pub trait Gate {
    /// Given a [`NoPrompt`] system.
    fn check(&self, sys: &dyn System) -> GateResult;
}

/// Work that runs with the real system after the steps.
pub type Hook<'a> = Box<dyn FnOnce(&dyn System) -> Result<()> + 'a>;

/// Runs once after the steps, if any of them changed something (e.g. reload a service).
pub struct AfterChange<'a> {
    pub name: String,
    pub notice: Option<String>,
    pub run: Hook<'a>,
}

pub struct Action<'a> {
    pub gate: Option<Box<dyn Gate + 'a>>,
    pub steps: Vec<Box<dyn Step + 'a>>,
    pub after_change: Option<AfterChange<'a>>,
}

/// Look, prompt only if needed, then act. Returns whether anything changed.
pub fn run(sys: &dyn System, action: Action) -> Result<bool> {
    let Action {
        gate,
        mut steps,
        after_change,
    } = action;
    let look = NoPrompt(sys);

    let mut needs = Vec::new();
    if let Some(g) = &gate {
        match g.check(&look) {
            GateResult::Pass => {}
            GateResult::Refuse(e) => return Err(e.into()),
            GateResult::Unknown(lines) => needs.extend(lines),
        }
    }
    let mut found = Vec::new();
    for step in &steps {
        let check = step.check(&look)?;
        if matches!(check, Check::NeedsChange(_) | Check::Unknown)
            && let Some(line) = step.notice(&check)
        {
            needs.push(line);
        }
        found.push(check);
    }
    let pending = found
        .iter()
        .any(|c| matches!(c, Check::NeedsChange(_) | Check::Unknown));

    if !pending && needs.is_empty() {
        for (step, check) in steps.iter().zip(&found) {
            print_ok(step.name(), check);
        }
        return Ok(false);
    }
    if pending && let Some(line) = after_change.as_ref().and_then(|a| a.notice.clone()) {
        needs.push(line);
    }
    if !needs.is_empty() {
        sudo_preflight(sys, &needs)?;
        if let Some(g) = &gate {
            match g.check(&look) {
                GateResult::Pass => {}
                GateResult::Refuse(e) => return Err(e.into()),
                GateResult::Unknown(lines) => {
                    bail!("sudo refused; could not {}", lines.join("; "))
                }
            }
        }
    }

    let changed = run_steps(sys, &mut steps)?;
    if changed && let Some(after) = after_change {
        (after.run)(sys)?;
        println!("changed: {}", after.name);
    }
    Ok(changed)
}

fn print_ok(name: &str, check: &Check) {
    match check {
        Check::DoneWith(note) => println!("ok: {name} ({note})"),
        _ => println!("ok: {name}"),
    }
}

/// Check and apply each step in order, printing `ok:`/`changed:`. On the first failure,
/// roll back the steps already applied (newest first) and return the error.
fn run_steps(sys: &dyn System, steps: &mut [Box<dyn Step + '_>]) -> Result<bool> {
    let look = NoPrompt(sys);
    let mut applied: Vec<usize> = Vec::new();
    for i in 0..steps.len() {
        let result = match steps[i].check(&look) {
            Ok(check @ (Check::Done | Check::DoneWith(_))) => {
                print_ok(steps[i].name(), &check);
                continue;
            }
            Ok(Check::NeedsChange(_)) => steps[i].apply(sys),
            Ok(Check::Unknown) => Err(anyhow!("could not check it: sudo refused")),
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

/// The system as checks see it: reads and `sudo -n` only. Anything that could prompt or
/// change the machine is an error, so a check that would do so fails loudly in tests.
pub struct NoPrompt<'a>(pub &'a dyn System);

impl System for NoPrompt<'_> {
    fn run(&self, cmd: &Cmd) -> Result<Output> {
        if cmd.root == Root::Sudo || cmd.interactive || cmd.stdin.is_some() {
            bail!(
                "check ran `{}`; checks may only read (use sudo -n)",
                cmd.line()
            );
        }
        self.0.run(cmd)
    }

    fn read_file(&self, path: &std::path::Path) -> Option<String> {
        self.0.read_file(path)
    }

    fn install_file(&self, path: &std::path::Path, _content: &str) -> Result<()> {
        bail!("check tried to write {}", path.display())
    }

    fn remove_file(&self, path: &std::path::Path) -> Result<()> {
        bail!("check tried to remove {}", path.display())
    }

    fn dir_owner(&self, path: &std::path::Path) -> Option<u32> {
        self.0.dir_owner(path)
    }

    fn current_user(&self) -> Option<String> {
        self.0.current_user()
    }

    fn prompt_password(&self, _prompt: &str) -> Result<String> {
        bail!("check tried to prompt for a password")
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::system::fake::FakeSystem;

    /// A step whose check result is fixed; `apply` runs `cmd` with sudo.
    struct Fixed {
        found: fn() -> Check,
        root: bool,
        applied: Cell<bool>,
    }

    impl Fixed {
        fn new(found: fn() -> Check, root: bool) -> Box<Self> {
            Box::new(Fixed {
                found,
                root,
                applied: Cell::new(false),
            })
        }
    }

    impl Step for Fixed {
        fn name(&self) -> &str {
            "fixed"
        }
        fn check(&self, _sys: &dyn System) -> Result<Check> {
            Ok(if self.applied.get() {
                Check::Done
            } else {
                (self.found)()
            })
        }
        fn notice(&self, _found: &Check) -> Option<String> {
            self.root.then(|| "do it".to_string())
        }
        fn apply(&mut self, sys: &dyn System) -> Result<()> {
            self.applied.set(true);
            let cmd = Cmd::new("true");
            sys.run(&if self.root { cmd.sudo() } else { cmd })?;
            Ok(())
        }
    }

    struct Refusing;
    impl Gate for Refusing {
        fn check(&self, _sys: &dyn System) -> GateResult {
            GateResult::Refuse(ExitError::new(ExitError::NOT_SET_UP, "not set up"))
        }
    }

    struct SneakySudo;
    impl Step for SneakySudo {
        fn name(&self) -> &str {
            "sneaky"
        }
        fn check(&self, sys: &dyn System) -> Result<Check> {
            sys.run(&Cmd::new("pdbedit").sudo())?;
            Ok(Check::Done)
        }
        fn notice(&self, _found: &Check) -> Option<String> {
            None
        }
        fn apply(&mut self, _sys: &dyn System) -> Result<()> {
            Ok(())
        }
    }

    fn action<'a>(steps: Vec<Box<dyn Step + 'a>>) -> Action<'a> {
        Action {
            gate: None,
            steps,
            after_change: None,
        }
    }

    #[test]
    fn nothing_to_do_never_prompts() {
        let sys = FakeSystem::new("me");
        let changed = run(&sys, action(vec![Fixed::new(|| Check::Done, true)])).unwrap();
        assert!(!changed);
        assert!(!sys.prompted());
    }

    #[test]
    fn change_without_root_never_prompts() {
        let sys = FakeSystem::new("me");
        let steps: Vec<Box<dyn Step>> = vec![Fixed::new(|| Check::NeedsChange("x".into()), false)];
        assert!(run(&sys, action(steps)).unwrap());
        assert!(!sys.prompted());
    }

    #[test]
    fn change_with_root_prompts_before_any_root_command() {
        let sys = FakeSystem::new("me");
        let steps: Vec<Box<dyn Step>> = vec![Fixed::new(|| Check::NeedsChange("x".into()), true)];
        assert!(run(&sys, action(steps)).unwrap());
        assert_eq!(sys.calls(), ["sudo -v", "sudo true"]);
    }

    #[test]
    fn refusing_gate_stops_before_prompting() {
        let sys = FakeSystem::new("me");
        let mut a = action(vec![Fixed::new(|| Check::NeedsChange("x".into()), true)]);
        a.gate = Some(Box::new(Refusing));
        let err = run(&sys, a).unwrap_err();
        assert_eq!(crate::exit_code(&err), ExitError::NOT_SET_UP);
        assert!(sys.calls().is_empty());
    }

    #[test]
    fn unknown_still_unknown_after_prompt_is_an_error() {
        let sys = FakeSystem::new("me");
        let steps: Vec<Box<dyn Step>> = vec![Fixed::new(|| Check::Unknown, true)];
        assert!(run(&sys, action(steps)).is_err());
        assert!(sys.prompted());
    }

    #[test]
    fn a_check_that_would_prompt_fails() {
        let sys = FakeSystem::new("me");
        let err = run(&sys, action(vec![Box::new(SneakySudo)])).unwrap_err();
        assert!(format!("{err:#}").contains("checks may only read"));
        assert!(sys.calls().is_empty());
    }
}
