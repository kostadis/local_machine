//! Access to the host: running programs, reading and installing files, prompting.
//!
//! Everything that touches the machine goes through [`System`], so actions can be tested
//! against [`fake::FakeSystem`] without changing the real machine.

pub mod action;
pub mod fake;
pub mod files;

use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::{Command, Stdio};

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
    println!("needs administrator privileges for:");
    for s in steps {
        println!("  {s}");
    }
    // Our own prompt: sudo-rs's default `[sudo: authenticate] Password:` is easy to miss.
    // `%` is sudo's prompt escape, so it is doubled in the user name.
    let user = sys.current_user().unwrap_or_default().replace('%', "%%");
    let prompt = format!("\n>>> sudo password for {user} (needed for the steps above): ");
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
}
