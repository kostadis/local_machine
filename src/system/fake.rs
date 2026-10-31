//! An in-memory [`System`] for tests: records commands, serves scripted outputs.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::{Cmd, Output, System};

#[derive(Default)]
pub struct FakeSystem {
    /// Command line → output. Unlisted commands succeed with empty output.
    pub responses: RefCell<HashMap<String, Output>>,
    pub calls: RefCell<Vec<String>>,
    pub files: RefCell<HashMap<PathBuf, String>>,
    pub dirs: RefCell<HashMap<PathBuf, u32>>,
    pub passwords: RefCell<VecDeque<String>>,
    pub user: Option<String>,
}

impl FakeSystem {
    pub fn new(user: &str) -> Self {
        FakeSystem {
            user: Some(user.to_string()),
            ..Default::default()
        }
    }

    pub fn respond(&self, line: &str, out: Output) {
        self.responses.borrow_mut().insert(line.to_string(), out);
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

    pub fn push_password(&self, pw: &str) {
        self.passwords.borrow_mut().push_back(pw.to_string());
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    pub fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
    }

    /// Calls that ran with root (`sudo ...`), excluding `sudo -v`.
    pub fn root_calls(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.starts_with("sudo ") && c != "sudo -v")
            .collect()
    }
}

impl System for FakeSystem {
    fn run(&self, cmd: &Cmd) -> Result<Output> {
        let line = cmd.line();
        self.calls.borrow_mut().push(line.clone());
        Ok(self
            .responses
            .borrow()
            .get(&line)
            .cloned()
            .unwrap_or_default())
    }

    fn read_file(&self, path: &Path) -> Option<String> {
        self.files.borrow().get(path).cloned()
    }

    fn install_file(&self, path: &Path, content: &str) -> Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("install {}", path.display()));
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        self.calls
            .borrow_mut()
            .push(format!("remove {}", path.display()));
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
}
