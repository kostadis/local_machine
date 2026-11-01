//! Access to the host: running programs, reading and installing files, prompting.
//!
//! Everything that touches the machine goes through [`System`], so actions can be tested
//! against [`fake::FakeSystem`] without changing the real machine.

pub mod action;
pub mod fake;
pub mod files;
pub mod net;
pub mod ufw;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};

/// How a command gets root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Root {
    /// Run as the invoking user.
    No,
    /// `sudo <cmd>`; may prompt, so only after [`sudo_preflight`].
    Sudo,
    /// `sudo -n <cmd>`; never prompts, fails instead.
    SudoNonInteractive,
}

#[derive(Clone, Debug)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    /// Written to the command's stdin. Never logged.
    pub stdin: Option<String>,
    pub root: Root,
    /// Inherit the terminal instead of capturing output (prompts, progress).
    pub interactive: bool,
}

impl Cmd {
    pub fn new(program: &str) -> Self {
        Cmd {
            program: program.to_string(),
            args: Vec::new(),
            stdin: None,
            root: Root::No,
            interactive: false,
        }
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.args
            .extend(args.into_iter().map(|a| a.as_ref().to_string()));
        self
    }

    pub fn sudo(mut self) -> Self {
        self.root = Root::Sudo;
        self
    }

    pub fn sudo_n(mut self) -> Self {
        self.root = Root::SudoNonInteractive;
        self
    }

    pub fn stdin(mut self, input: String) -> Self {
        self.stdin = Some(input);
        self
    }

    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }

    /// The command line as it would be typed, without stdin.
    pub fn line(&self) -> String {
        let prefix = match self.root {
            Root::No => "",
            Root::Sudo => "sudo ",
            Root::SudoNonInteractive => "sudo -n ",
        };
        let mut line = format!("{prefix}{}", self.program);
        for a in &self.args {
            line.push(' ');
            line.push_str(a);
        }
        line
    }
}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    pub fn ok(stdout: &str) -> Self {
        Output {
            code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    pub fn fail(code: i32, stderr: &str) -> Self {
        Output {
            code,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }
}

/// What is at a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    Missing,
    File,
    Dir,
    Other,
}

pub trait System {
    fn run(&self, cmd: &Cmd) -> Result<Output>;
    fn read_file(&self, path: &Path) -> Option<String>;
    /// Atomically replace a root-owned file with `content` (mode 0644).
    fn install_file(&self, path: &Path, content: &str) -> Result<()>;
    fn remove_file(&self, path: &Path) -> Result<()>;
    /// Owner uid of `path` if it is a directory.
    fn dir_owner(&self, path: &Path) -> Option<u32>;
    fn current_user(&self) -> Option<String>;
    fn prompt_password(&self, prompt: &str) -> Result<String>;

    // Owner-level operations: none of these use root.

    fn path_kind(&self, path: &Path) -> PathKind;
    /// Names of the entries directly inside `dir` (empty when it cannot be read).
    fn list_dir(&self, dir: &Path) -> Vec<String>;
    fn make_dir_all(&self, path: &Path) -> Result<()>;
    /// Atomically write an owner-owned file: a temp file in the same directory, then rename.
    fn write_user_file(&self, path: &Path, content: &str) -> Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn copy_file(&self, from: &Path, to: &Path) -> Result<()>;
    /// Remove an owner-owned file (no error when it is already gone).
    fn remove_user_file(&self, path: &Path) -> Result<()>;
    fn remove_dir_all(&self, path: &Path) -> Result<()>;
    /// HTTP status of `GET <path>` on `127.0.0.1:<port>`; `None` on no answer in `timeout`.
    fn http_status(&self, port: u16, path: &str, timeout: Duration) -> Option<u16>;
    fn effective_uid(&self) -> u32;
    fn home_dir(&self) -> Option<PathBuf>;
    fn sleep(&self, duration: Duration);
}

/// Whether `cmd` could be run and exited 0.
pub fn runs(sys: &dyn System, cmd: &Cmd) -> bool {
    sys.run(cmd).is_ok_and(|o| o.success())
}

/// The status code from an HTTP response's first line (`HTTP/1.1 200 OK` → 200).
pub fn parse_status_line(response: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(response);
    let first = text.lines().next()?;
    let mut parts = first.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

/// Run a command and fail unless it exits 0.
pub fn run_ok(sys: &dyn System, cmd: &Cmd) -> Result<Output> {
    let out = sys.run(cmd)?;
    if !out.success() {
        let detail = out.stderr.trim();
        if detail.is_empty() {
            bail!("`{}` exited with {}", cmd.line(), out.code);
        }
        bail!("`{}` exited with {}: {detail}", cmd.line(), out.code);
    }
    Ok(out)
}

/// Say which privileged steps may follow, then cache sudo credentials (FR-009).
/// Actions call this through [`action::run`], which decides whether it is needed.
pub fn sudo_preflight(sys: &dyn System, steps: &[String]) -> Result<()> {
    crate::say!("needs administrator privileges for:");
    for s in steps {
        crate::say!("  {s}");
    }
    // Credentials still cached: nothing to ask, so no password line.
    if sys.run(&Cmd::new("sudo").args(["-n", "-v"]))?.success() {
        return Ok(());
    }
    // sudo-rs shows a prompt as `[sudo: <prompt>] Password:`, easy to miss under the notice,
    // so say it on a line of our own first. `%` is sudo's prompt escape.
    let user = sys.current_user().unwrap_or_default();
    crate::say!("\n>>> enter your sudo password for {user} (needed for the steps above)");
    let prompt = format!("password for {}", user.replace('%', "%%"));
    let out = sys.run(&Cmd::new("sudo").args(["-v", "-p", &prompt]).interactive())?;
    if !out.success() {
        bail!("administrator privileges refused");
    }
    Ok(())
}

/// The real machine.
pub struct RealSystem;

impl System for RealSystem {
    fn run(&self, cmd: &Cmd) -> Result<Output> {
        let mut command = match cmd.root {
            Root::No => Command::new(&cmd.program),
            Root::Sudo => {
                let mut c = Command::new("sudo");
                c.arg(&cmd.program);
                c
            }
            Root::SudoNonInteractive => {
                let mut c = Command::new("sudo");
                c.arg("-n").arg(&cmd.program);
                c
            }
        };
        command.args(&cmd.args);
        if cmd.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else if !cmd.interactive {
            command.stdin(Stdio::null());
        }
        if !cmd.interactive {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("cannot run `{}`", cmd.line()))?;
        if let Some(input) = &cmd.stdin {
            let mut pipe = child.stdin.take().expect("stdin was piped");
            pipe.write_all(input.as_bytes())
                .with_context(|| format!("cannot write to `{}`", cmd.line()))?;
        }
        let out = child
            .wait_with_output()
            .with_context(|| format!("cannot wait for `{}`", cmd.line()))?;
        Ok(Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    fn read_file(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn install_file(&self, path: &Path, content: &str) -> Result<()> {
        let mut tmp = tempfile::NamedTempFile::new().context("cannot create temp file")?;
        tmp.write_all(content.as_bytes())
            .context("cannot write temp file")?;
        let staged = format!("{}.local-machine.new", path.display());
        let tmp_path = tmp.path().display().to_string();
        run_ok(
            self,
            &Cmd::new("install")
                .args(["-m", "0644", &tmp_path, &staged])
                .sudo(),
        )?;
        let target = path.display().to_string();
        run_ok(self, &Cmd::new("mv").args(["-f", &staged, &target]).sudo())?;
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        let target = path.display().to_string();
        run_ok(self, &Cmd::new("rm").args(["-f", &target]).sudo())?;
        Ok(())
    }

    fn dir_owner(&self, path: &Path) -> Option<u32> {
        let meta = std::fs::metadata(path).ok()?;
        meta.is_dir().then(|| meta.uid())
    }

    fn current_user(&self) -> Option<String> {
        std::env::var("USER").ok().filter(|u| !u.is_empty())
    }

    fn prompt_password(&self, prompt: &str) -> Result<String> {
        rpassword::prompt_password(prompt).context("cannot read password")
    }

    fn path_kind(&self, path: &Path) -> PathKind {
        match std::fs::metadata(path) {
            Err(_) => PathKind::Missing,
            Ok(m) if m.is_file() => PathKind::File,
            Ok(m) if m.is_dir() => PathKind::Dir,
            Ok(_) => PathKind::Other,
        }
    }

    fn list_dir(&self, dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn make_dir_all(&self, path: &Path) -> Result<()> {
        std::fs::create_dir_all(path).with_context(|| format!("cannot create {}", path.display()))
    }

    fn write_user_file(&self, path: &Path, content: &str) -> Result<()> {
        let dir = path
            .parent()
            .with_context(|| format!("{} has no parent directory", path.display()))?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir)
            .with_context(|| format!("cannot create a temp file in {}", dir.display()))?;
        tmp.write_all(content.as_bytes())
            .with_context(|| format!("cannot write {}", path.display()))?;
        tmp.persist(path)
            .with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        std::fs::rename(from, to)
            .with_context(|| format!("cannot rename {} to {}", from.display(), to.display()))
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<()> {
        std::fs::copy(from, to)
            .map(|_| ())
            .with_context(|| format!("cannot copy {} to {}", from.display(), to.display()))
    }

    fn remove_user_file(&self, path: &Path) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("cannot remove {}", path.display())),
        }
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        std::fs::remove_dir_all(path).with_context(|| format!("cannot remove {}", path.display()))
    }

    fn http_status(&self, port: u16, path: &str, timeout: Duration) -> Option<u16> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
        stream.set_read_timeout(Some(timeout)).ok()?;
        stream.set_write_timeout(Some(timeout)).ok()?;
        let request = format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n");
        stream.write_all(request.as_bytes()).ok()?;
        let mut buf = [0u8; 256];
        let mut got = Vec::new();
        while !got.contains(&b'\n') && got.len() < 4096 {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
            }
        }
        parse_status_line(&got)
    }

    fn effective_uid(&self) -> u32 {
        // /proc/self/status: `Uid: <real> <effective> <saved> <fs>`.
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("Uid:"))
                    .and_then(|l| l.split_whitespace().nth(2))
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(u32::MAX)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_parsing() {
        assert_eq!(
            parse_status_line(b"HTTP/1.1 200 OK\r\nServer: x\r\n"),
            Some(200)
        );
        assert_eq!(parse_status_line(b"HTTP/1.0 404 Not Found\r\n"), Some(404));
        assert_eq!(parse_status_line(b"HTTP/1.1 503\r\n"), Some(503));
        assert_eq!(parse_status_line(b"SSH-2.0-OpenSSH\r\n"), None);
        assert_eq!(parse_status_line(b""), None);
    }
}
