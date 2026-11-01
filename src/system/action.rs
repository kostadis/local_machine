//! Privileged actions, shared by every subcommand: look without prompting, then act.
//!
//! An action is an optional [`Gate`] (what must already be true) and a list of [`Step`]s
//! (what to make true). [`run`] checks everything first through [`NoPrompt`], so checks can
//! only use `sudo -n`. It prompts for sudo only when a step needs root to change something
//! or to find out, and its privilege notice names only those steps. When everything is
//! already in place it prints `ok:` lines and never prompts.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};

use super::{Cmd, Output, PathKind, Root, System, sudo_preflight};
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
    /// An independent step (e.g. one restore): when it fails, report it, keep going with
    /// the rest, and fail the action at the end without rolling anything back.
    fn continue_on_failure(&self) -> bool {
        false
    }
}

/// Why [`run`] failed partway through its steps.
#[derive(Debug)]
pub struct StepFailure {
    pub message: String,
    /// Names of the steps that are in place (already done or changed by this run).
    pub done: Vec<String>,
    /// Names of the steps not reached because of the failure.
    pub remaining: Vec<String>,
}

impl fmt::Display for StepFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StepFailure {}

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
        crate::say!("changed: {}", after.name);
    }
    Ok(changed)
}

fn print_ok(name: &str, check: &Check) {
    match check {
        Check::DoneWith(note) => crate::say!("ok: {name} ({note})"),
        _ => crate::say!("ok: {name}"),
    }
}

/// Check and apply each step in order, printing `ok:`, or `adding:` then `changed:`. On the
/// first failure of an ordinary step, roll back the steps already applied (newest first)
/// and return the error. A failing step that continues on failure prints `error:` and the
/// loop goes on; those failures are returned together at the end, nothing rolled back.
fn run_steps(sys: &dyn System, steps: &mut [Box<dyn Step + '_>]) -> Result<bool> {
    let look = NoPrompt(sys);
    let mut applied: Vec<usize> = Vec::new();
    let mut done: Vec<String> = Vec::new();
    let mut tolerated: Vec<String> = Vec::new();
    for i in 0..steps.len() {
        let result = match steps[i].check(&look) {
            Ok(check @ (Check::Done | Check::DoneWith(_))) => {
                print_ok(steps[i].name(), &check);
                done.push(steps[i].name().to_string());
                continue;
            }
            Ok(Check::NeedsChange(_)) => {
                crate::say!("adding: {}", steps[i].name());
                steps[i].apply(sys)
            }
            Ok(Check::Unknown) => Err(anyhow!("could not check it: sudo refused")),
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => {
                crate::say!("changed: {}", steps[i].name());
                applied.push(i);
                done.push(steps[i].name().to_string());
            }
            Err(e) if steps[i].continue_on_failure() => {
                crate::say!("error: {}: {e:#}", steps[i].name());
                tolerated.push(steps[i].name().to_string());
            }
            Err(e) => {
                for &j in applied.iter().rev() {
                    if let Err(re) = steps[j].rollback(sys) {
                        eprintln!("error: rolling back {}: {re:#}", steps[j].name());
                    }
                }
                return Err(StepFailure {
                    message: format!("{}: {e:#}", steps[i].name()),
                    done,
                    remaining: steps[i..].iter().map(|s| s.name().to_string()).collect(),
                }
                .into());
            }
        }
    }
    if !tolerated.is_empty() {
        let n = tolerated.len();
        return Err(StepFailure {
            message: format!(
                "{n} step{} failed: {}",
                if n == 1 { "" } else { "s" },
                tolerated.join(", ")
            ),
            done,
            remaining: tolerated,
        }
        .into());
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

    fn path_kind(&self, path: &Path) -> PathKind {
        self.0.path_kind(path)
    }

    fn list_dir(&self, dir: &Path) -> Vec<String> {
        self.0.list_dir(dir)
    }

    fn make_dir_all(&self, path: &Path) -> Result<()> {
        bail!("check tried to create {}", path.display())
    }

    fn write_user_file(&self, path: &Path, _content: &str) -> Result<()> {
        bail!("check tried to write {}", path.display())
    }

    fn rename(&self, from: &Path, _to: &Path) -> Result<()> {
        bail!("check tried to rename {}", from.display())
    }

    fn copy_file(&self, _from: &Path, to: &Path) -> Result<()> {
        bail!("check tried to write {}", to.display())
    }

    fn remove_user_file(&self, path: &Path) -> Result<()> {
        bail!("check tried to remove {}", path.display())
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        bail!("check tried to remove {}", path.display())
    }

    fn http_status(&self, port: u16, path: &str, timeout: Duration) -> Option<u16> {
        self.0.http_status(port, path, timeout)
    }

    fn effective_uid(&self) -> u32 {
        self.0.effective_uid()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.0.home_dir()
    }

    fn sleep(&self, duration: Duration) {
        self.0.sleep(duration)
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
        assert!(sys.prompted());
        assert_eq!(sys.root_calls(), ["sudo true"]);
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

    /// A named owner-level step: `fails` makes `apply` fail; `tolerant` continues on failure.
    struct Named {
        name: &'static str,
        done: bool,
        fails: bool,
        tolerant: bool,
    }

    impl Named {
        fn new(name: &'static str, done: bool, fails: bool, tolerant: bool) -> Box<Self> {
            Box::new(Named {
                name,
                done,
                fails,
                tolerant,
            })
        }
    }

    impl Step for Named {
        fn name(&self) -> &str {
            self.name
        }
        fn check(&self, _sys: &dyn System) -> Result<Check> {
            Ok(if self.done {
                Check::Done
            } else {
                Check::NeedsChange("x".into())
            })
        }
        fn notice(&self, _found: &Check) -> Option<String> {
            None
        }
        fn apply(&mut self, sys: &dyn System) -> Result<()> {
            sys.run(&Cmd::new("apply").args([self.name]))?;
            if self.fails {
                bail!("broke");
            }
            Ok(())
        }
        fn continue_on_failure(&self) -> bool {
            self.tolerant
        }
    }

    fn failure(err: &anyhow::Error) -> &StepFailure {
        err.downcast_ref::<StepFailure>().expect("a StepFailure")
    }

    #[test]
    fn tolerant_failures_are_collected_and_successes_kept() {
        let sys = FakeSystem::new("me");
        let steps: Vec<Box<dyn Step>> = vec![
            Named::new("a", false, true, true),
            Named::new("b", false, false, true),
            Named::new("c", false, true, true),
        ];
        let err = run(&sys, action(steps)).unwrap_err();
        let f = failure(&err);
        assert_eq!(f.message, "2 steps failed: a, c");
        assert_eq!(f.done, ["b"]);
        assert_eq!(f.remaining, ["a", "c"]);
        assert_eq!(sys.calls(), ["apply a", "apply b", "apply c"]);
    }

    #[test]
    fn an_intolerant_failure_after_a_tolerant_one_stops_at_once() {
        let sys = FakeSystem::new("me");
        let steps: Vec<Box<dyn Step>> = vec![
            Named::new("a", false, true, true),
            Named::new("b", false, true, false),
            Named::new("c", false, false, false),
        ];
        let err = run(&sys, action(steps)).unwrap_err();
        let f = failure(&err);
        assert_eq!(f.message, "b: broke");
        assert_eq!(f.remaining, ["b", "c"]);
        assert_eq!(sys.calls(), ["apply a", "apply b"]);
    }

    #[test]
    fn done_steps_print_ok_and_changed_steps_print_adding_first() {
        let sys = FakeSystem::new("me");
        let steps: Vec<Box<dyn Step>> = vec![
            Named::new("a", true, false, false),
            Named::new("b", false, false, false),
        ];
        let (result, lines) = crate::output::capture(|| run(&sys, action(steps)));
        assert!(result.unwrap());
        assert_eq!(lines, ["ok: a", "adding: b", "changed: b"]);
    }

    #[test]
    fn a_failed_step_reports_what_is_done_and_what_remains() {
        let sys = FakeSystem::new("me");
        let steps: Vec<Box<dyn Step>> = vec![
            Named::new("a", true, false, false),
            Named::new("b", false, true, false),
            Named::new("c", false, false, false),
        ];
        let err = run(&sys, action(steps)).unwrap_err();
        assert_eq!(failure(&err).done, ["a"]);
        assert_eq!(failure(&err).remaining, ["b", "c"]);
    }

    #[test]
    fn a_check_that_would_prompt_fails() {
        let sys = FakeSystem::new("me");
        let err = run(&sys, action(vec![Box::new(SneakySudo)])).unwrap_err();
        assert!(format!("{err:#}").contains("checks may only read"));
        assert!(sys.calls().is_empty());
    }
}
