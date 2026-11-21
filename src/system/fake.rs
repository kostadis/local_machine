//! An in-memory [`System`] for tests: records commands, serves scripted outputs.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};

use super::{Cmd, Output, PathKind, System};

/// Runs when a command line is issued, to change what later checks see.
pub type Effect = Box<dyn Fn(&FakeSystem)>;
/// Like [`Effect`], given the full command line.
/// A probed `(host, port)`.
pub type HostPort = (String, u16);
pub type LineEffect = Box<dyn Fn(&FakeSystem, &str)>;
/// Computes a command's output from the fake's current state.
pub type Responder = Box<dyn Fn(&FakeSystem, &str) -> Output>;

#[derive(Default)]
pub struct FakeSystem {
    /// Command line → output. Checked first.
    pub responses: RefCell<HashMap<String, Output>>,
    /// Command-line prefix → output, checked in insertion order after `responses`.
    pub prefixes: RefCell<Vec<(String, Output)>>,
    /// Output for commands matched by neither (default: success, empty output).
    pub fallback: RefCell<Output>,
    /// Command line → effect, run after the command is recorded.
    pub effects: RefCell<HashMap<String, Effect>>,
    /// Command-line prefix → effect, run after the exact effect (all that match).
    pub prefix_effects: RefCell<Vec<(String, std::rc::Rc<LineEffect>)>>,
    /// Command-line prefix → output computed from state; checked before everything else.
    pub responders: RefCell<Vec<(String, std::rc::Rc<Responder>)>>,
    pub calls: RefCell<Vec<String>>,
    /// Environment of each recorded command, parallel to `calls`.
    pub envs: RefCell<Vec<Vec<(String, String)>>>,
    /// (host, port) → answers to successive `http_status_at` probes; the last one repeats.
    pub http_at: RefCell<HashMap<HostPort, VecDeque<Option<u16>>>>,
    /// Scripted symlinks: path → its canonical form.
    pub canon: RefCell<HashMap<PathBuf, PathBuf>>,
    pub files: RefCell<HashMap<PathBuf, String>>,
    pub dirs: RefCell<HashMap<PathBuf, u32>>,
    pub passwords: RefCell<VecDeque<String>>,
    /// Port → answers to successive probes; the last one repeats.
    pub http: RefCell<HashMap<u16, VecDeque<Option<u16>>>>,
    pub euid: Cell<u32>,
    pub home: RefCell<Option<PathBuf>>,
    pub user: Option<String>,
    /// Paths whose `remove_dir_all` fails (scripted by `fail_remove`).
    pub remove_failures: RefCell<Vec<PathBuf>>,
}

impl FakeSystem {
    /// Starts with no cached sudo credentials, so the privilege notice prompts.
    pub fn new(user: &str) -> Self {
        let sys = FakeSystem {
            user: Some(user.to_string()),
            euid: Cell::new(1000),
            home: RefCell::new(Some(PathBuf::from(format!("/home/{user}")))),
            ..Default::default()
        };
        sys.respond("sudo -n -v", Output::fail(1, "a password is required"));
        sys
    }

    pub fn respond(&self, line: &str, out: Output) {
        self.responses.borrow_mut().insert(line.to_string(), out);
    }

    /// Every command line starting with `prefix` (and not listed exactly) gets `out`.
    pub fn respond_prefix(&self, prefix: &str, out: Output) {
        let mut p = self.prefixes.borrow_mut();
        p.retain(|(k, _)| k != prefix);
        p.push((prefix.to_string(), out));
    }

    /// Output for every command that has no scripted response.
    pub fn set_fallback(&self, out: Output) {
        *self.fallback.borrow_mut() = out;
    }

    /// Every command line starting with `prefix` runs `effect` after it is issued.
    pub fn on_prefix(&self, prefix: &str, effect: impl Fn(&FakeSystem, &str) + 'static) {
        self.prefix_effects
            .borrow_mut()
            .push((prefix.to_string(), std::rc::Rc::new(Box::new(effect))));
    }

    /// Every command line starting with `prefix` gets the output `f` computes at that time.
    /// The most recently added matching responder wins; responders win over scripted
    /// outputs.
    pub fn respond_fn(&self, prefix: &str, f: impl Fn(&FakeSystem, &str) -> Output + 'static) {
        self.responders
            .borrow_mut()
            .insert(0, (prefix.to_string(), std::rc::Rc::new(Box::new(f))));
    }

    pub fn on_run(&self, line: &str, effect: impl Fn(&FakeSystem) + 'static) {
        self.effects
            .borrow_mut()
            .insert(line.to_string(), Box::new(effect));
    }

    pub fn set_file(&self, path: &str, content: &str) {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(path), content.to_string());
    }

    pub fn file(&self, path: &str) -> Option<String> {
        self.files.borrow().get(Path::new(path)).cloned()
    }

    pub fn set_dir(&self, path: &str, uid: u32) {
        self.dirs.borrow_mut().insert(PathBuf::from(path), uid);
    }

    pub fn has_dir(&self, path: &str) -> bool {
        self.dirs.borrow().contains_key(Path::new(path))
    }

    /// Scripted answers for successive probes of `port`; the last one repeats.
    pub fn set_http(&self, port: u16, answers: &[Option<u16>]) {
        self.http
            .borrow_mut()
            .insert(port, answers.iter().copied().collect());
    }

    /// Scripted answers for successive probes of `host:port`; the last one repeats.
    pub fn set_http_at(&self, host: &str, port: u16, answers: &[Option<u16>]) {
        self.http_at
            .borrow_mut()
            .insert((host.to_string(), port), answers.iter().copied().collect());
    }

    /// Script `from` to resolve to `to` (a symlink).
    pub fn set_canonical(&self, from: &str, to: &str) {
        self.canon
            .borrow_mut()
            .insert(PathBuf::from(from), PathBuf::from(to));
    }

    /// The environment of the last `run` whose line starts with `line_prefix`.
    pub fn env_of(&self, line_prefix: &str) -> Option<Vec<(String, String)>> {
        let calls = self.calls.borrow();
        let envs = self.envs.borrow();
        // `calls` also holds non-run records, so match on the run records only.
        calls
            .iter()
            .enumerate()
            .rev()
            .find(|(_, c)| c.starts_with(line_prefix))
            .and_then(|(i, _)| envs.get(i).cloned())
    }

    /// Make `remove_dir_all(path)` fail (it is still recorded, and removes nothing).
    pub fn fail_remove(&self, path: &str) {
        self.remove_failures.borrow_mut().push(PathBuf::from(path));
    }

    pub fn set_euid(&self, uid: u32) {
        self.euid.set(uid);
    }

    pub fn push_password(&self, pw: &str) {
        self.passwords.borrow_mut().push_back(pw.to_string());
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    pub fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
        self.envs.borrow_mut().clear();
    }

    fn record(&self, line: String) {
        self.record_env(line, Vec::new());
    }

    fn record_env(&self, line: String, env: Vec<(String, String)>) {
        self.calls.borrow_mut().push(line);
        self.envs.borrow_mut().push(env);
    }

    /// Whether the privilege notice prompted (`sudo -v`). Panics if any command ran with
    /// full root before it: the rule every privileged action must keep.
    pub fn prompted(&self) -> bool {
        let calls = self.calls();
        let notice = calls.iter().position(|c| is_notice(c));
        let first_root = calls
            .iter()
            .position(|c| c.starts_with("sudo ") && !c.starts_with("sudo -n ") && !is_notice(c));
        if let Some(r) = first_root {
            assert!(
                notice.is_some_and(|n| n < r),
                "`{}` ran with root before the privilege notice; calls: {calls:#?}",
                calls[r]
            );
        }
        notice.is_some()
    }

    /// Calls that ran with root (`sudo ...`), excluding the `sudo -v` prompt.
    pub fn root_calls(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.starts_with("sudo ") && !is_notice(c))
            .collect()
    }

    fn output_for(&self, line: &str) -> Output {
        if let Some(out) = self.responses.borrow().get(line) {
            return out.clone();
        }
        for (prefix, out) in self.prefixes.borrow().iter() {
            if line.starts_with(prefix.as_str()) {
                return out.clone();
            }
        }
        self.fallback.borrow().clone()
    }
}

/// The privilege notice's credential check: `sudo -n -v` (cached?), then `sudo -v`, with or
/// without a custom prompt.
fn is_notice(call: &str) -> bool {
    call == "sudo -n -v" || call == "sudo -v" || call.starts_with("sudo -v ")
}

impl System for FakeSystem {
    fn run(&self, cmd: &Cmd) -> Result<Output> {
        let line = cmd.line();
        self.record_env(line.clone(), cmd.env.clone());
        let responder = self
            .responders
            .borrow()
            .iter()
            .find(|(p, _)| line.starts_with(p.as_str()))
            .map(|(_, f)| f.clone());
        let out = match responder {
            Some(f) => f(self, &line),
            None => self.output_for(&line),
        };
        // Take the effect out while it runs, so it may script further effects.
        let effect = self.effects.borrow_mut().remove(&line);
        if let Some(effect) = effect {
            effect(self);
            self.effects
                .borrow_mut()
                .entry(line.clone())
                .or_insert(effect);
        }
        let matching: Vec<std::rc::Rc<LineEffect>> = self
            .prefix_effects
            .borrow()
            .iter()
            .filter(|(p, _)| line.starts_with(p.as_str()))
            .map(|(_, f)| f.clone())
            .collect();
        for f in matching {
            f(self, &line);
        }
        Ok(out)
    }

    fn read_file(&self, path: &Path) -> Option<String> {
        self.files.borrow().get(path).cloned()
    }

    fn install_file(&self, path: &Path, content: &str) -> Result<()> {
        self.record(format!("install {}", path.display()));
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        self.record(format!("remove {}", path.display()));
        self.files.borrow_mut().remove(path);
        Ok(())
    }

    fn dir_owner(&self, path: &Path) -> Option<u32> {
        self.dirs.borrow().get(path).copied()
    }

    fn current_user(&self) -> Option<String> {
        self.user.clone()
    }

    fn prompt_password(&self, _prompt: &str) -> Result<String> {
        match self.passwords.borrow_mut().pop_front() {
            Some(pw) => Ok(pw),
            None => bail!("no password scripted"),
        }
    }

    fn path_kind(&self, path: &Path) -> PathKind {
        if self.files.borrow().contains_key(path) {
            PathKind::File
        } else if self.dirs.borrow().contains_key(path) {
            PathKind::Dir
        } else {
            PathKind::Missing
        }
    }

    fn list_dir(&self, dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = self
            .files
            .borrow()
            .keys()
            .chain(self.dirs.borrow().keys())
            .filter(|p| p.parent() == Some(dir))
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn make_dir_all(&self, path: &Path) -> Result<()> {
        self.record(format!("mkdir {}", path.display()));
        self.dirs
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_insert(self.euid.get());
        Ok(())
    }

    fn write_user_file(&self, path: &Path, content: &str) -> Result<()> {
        self.record(format!("write {}", path.display()));
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.record(format!("rename {} {}", from.display(), to.display()));
        let content = self.files.borrow_mut().remove(from);
        match content {
            Some(c) => {
                self.files.borrow_mut().insert(to.to_path_buf(), c);
                Ok(())
            }
            None => bail!("cannot rename {}: no such file", from.display()),
        }
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<()> {
        self.record(format!("copy {} {}", from.display(), to.display()));
        let content = self.files.borrow().get(from).cloned();
        match content {
            Some(c) => {
                self.files.borrow_mut().insert(to.to_path_buf(), c);
                Ok(())
            }
            None => bail!("cannot copy {}: no such file", from.display()),
        }
    }

    fn remove_user_file(&self, path: &Path) -> Result<()> {
        self.record(format!("rm {}", path.display()));
        self.files.borrow_mut().remove(path);
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        self.record(format!("rmdir {}", path.display()));
        if self.remove_failures.borrow().iter().any(|p| p == path) {
            bail!("permission denied");
        }
        self.dirs.borrow_mut().retain(|p, _| !p.starts_with(path));
        self.files.borrow_mut().retain(|p, _| !p.starts_with(path));
        Ok(())
    }

    fn http_status(&self, port: u16, _path: &str, _timeout: Duration) -> Option<u16> {
        let mut http = self.http.borrow_mut();
        let answers = http.get_mut(&port)?;
        if answers.len() > 1 {
            answers.pop_front().flatten()
        } else {
            answers.front().copied().flatten()
        }
    }

    fn http_status_at(
        &self,
        host: &str,
        port: u16,
        _path: &str,
        _timeout: Duration,
    ) -> Option<u16> {
        let mut http = self.http_at.borrow_mut();
        let answers = http.get_mut(&(host.to_string(), port))?;
        if answers.len() > 1 {
            answers.pop_front().flatten()
        } else {
            answers.front().copied().flatten()
        }
    }

    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        if let Some(to) = self.canon.borrow().get(path) {
            return Some(to.clone());
        }
        let exists =
            self.files.borrow().contains_key(path) || self.dirs.borrow().contains_key(path);
        exists.then(|| path.to_path_buf())
    }

    fn effective_uid(&self) -> u32 {
        self.euid.get()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home.borrow().clone()
    }

    fn sleep(&self, _duration: Duration) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_of_returns_last_matching_call_env() {
        let sys = FakeSystem::new("u");
        sys.run(&Cmd::new("tool").args(["a"]).env("K", "1"))
            .unwrap();
        sys.write_user_file(Path::new("/x"), "c").unwrap();
        sys.run(&Cmd::new("tool").args(["b"]).env("K", "2"))
            .unwrap();
        assert_eq!(
            sys.env_of("tool"),
            Some(vec![("K".to_string(), "2".to_string())])
        );
        assert_eq!(
            sys.env_of("tool a"),
            Some(vec![("K".to_string(), "1".to_string())])
        );
        assert_eq!(sys.env_of("nope"), None);
    }

    #[test]
    fn http_at_is_scripted_per_host_and_port() {
        let sys = FakeSystem::new("u");
        let t = Duration::from_secs(1);
        sys.set_http_at("h", 80, &[None, Some(200)]);
        assert_eq!(sys.http_status_at("h", 80, "/", t), None);
        assert_eq!(sys.http_status_at("h", 80, "/", t), Some(200));
        assert_eq!(sys.http_status_at("h", 80, "/", t), Some(200));
        assert_eq!(sys.http_status_at("other", 80, "/", t), None);
        assert_eq!(sys.http_status(80, "/", t), None);
    }

    #[test]
    fn canonical_identity_for_existing_scripted_links_else_none() {
        let sys = FakeSystem::new("u");
        sys.set_dir("/a", 1000);
        assert_eq!(sys.canonical(Path::new("/a")), Some(PathBuf::from("/a")));
        assert_eq!(sys.canonical(Path::new("/missing")), None);
        sys.set_canonical("/link", "/a");
        assert_eq!(sys.canonical(Path::new("/link")), Some(PathBuf::from("/a")));
    }
}
