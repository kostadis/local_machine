//! Git checkouts and sparse data trees (research R4). Created only when missing; an
//! existing checkout is never changed (FR-012), and an occupied path is never touched
//! (FR-013).

use std::path::Path;

use anyhow::{Result, bail};

use crate::system::action::{Check, Step};
use crate::system::{Cmd, PathKind, System, run_ok};

use super::config::Checkout;

/// Compare remotes: no trailing `.git` or `/`, and `git@github.com:x` ≡ `https://github.com/x`.
pub fn normalize_remote(url: &str) -> String {
    let mut u = url.trim().to_string();
    while u.ends_with('/') {
        u.pop();
    }
    if let Some(stripped) = u.strip_suffix(".git") {
        u = stripped.to_string();
    }
    if let Some(rest) = u.strip_prefix("git@")
        && let Some((host, path)) = rest.split_once(':')
    {
        u = format!("https://{host}/{path}");
    }
    if let Some(rest) = u.strip_prefix("ssh://git@") {
        u = format!("https://{rest}");
    }
    if let Some(rest) = u.strip_prefix("http://") {
        u = format!("https://{rest}");
    }
    u.to_ascii_lowercase()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    Present,
    Missing,
    /// What is there instead: `a file`, `not a git repository`, `origin is <url>`.
    Occupied(String),
}

pub fn inspect(sys: &dyn System, co: &Checkout) -> Found {
    match sys.path_kind(&co.path) {
        PathKind::Missing => return Found::Missing,
        PathKind::File | PathKind::Other => return Found::Occupied("a file".to_string()),
        PathKind::Dir => {}
    }
    if sys.path_kind(&co.path.join(".git")) == PathKind::Missing {
        return Found::Occupied("not a git repository".to_string());
    }
    let path = co.path.display().to_string();
    match sys.run(&Cmd::new("git").args(["-C", &path, "remote", "get-url", "origin"])) {
        Ok(o) if o.success() => {
            let origin = o.stdout.trim();
            if normalize_remote(origin) == normalize_remote(&co.repo) {
                Found::Present
            } else {
                Found::Occupied(format!("origin is {origin}"))
            }
        }
        _ => Found::Occupied("a git repository with no origin".to_string()),
    }
}

/// Clone `co`; on failure remove the directory this created (it did not exist before).
pub fn create(sys: &dyn System, co: &Checkout) -> Result<()> {
    let result = clone(sys, co);
    if result.is_err() && sys.path_kind(&co.path) != PathKind::Missing {
        sys.remove_dir_all(&co.path)?;
    }
    result
}

fn clone(sys: &dyn System, co: &Checkout) -> Result<()> {
    let path = co.path.display().to_string();
    if let Some(parent) = co.path.parent()
        && sys.path_kind(parent) == PathKind::Missing
    {
        sys.make_dir_all(parent)?;
    }
    match &co.sparse {
        None => {
            run_ok(
                sys,
                &Cmd::new("git")
                    .args(["clone", "--branch", &co.branch, &co.repo, &path])
                    .interactive(),
            )?;
        }
        Some(dirs) => {
            run_ok(
                sys,
                &Cmd::new("git")
                    .args([
                        "clone",
                        "--no-checkout",
                        "--branch",
                        &co.branch,
                        &co.repo,
                        &path,
                    ])
                    .interactive(),
            )?;
            run_ok(
                sys,
                &Cmd::new("git").args(["-C", &path, "sparse-checkout", "init", "--cone"]),
            )?;
            run_ok(
                sys,
                &Cmd::new("git")
                    .args(["-C", &path, "sparse-checkout", "set"])
                    .args(dirs),
            )?;
            run_ok(
                sys,
                &Cmd::new("git").args(["-C", &path, "checkout", &co.branch]),
            )?;
        }
    }
    Ok(())
}

/// `checkout <path>`: one per configured entry.
pub struct CheckoutStep {
    name: String,
    co: Checkout,
}

impl CheckoutStep {
    pub fn new(co: &Checkout) -> Self {
        CheckoutStep {
            name: format!("checkout {}", co.path.display()),
            co: co.clone(),
        }
    }
}

impl Step for CheckoutStep {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        Ok(match inspect(sys, &self.co) {
            Found::Present => Check::Done,
            Found::Missing => Check::NeedsChange(format!("{} is missing", self.co.path.display())),
            Found::Occupied(found) => Check::NeedsChange(format!("occupied ({found})")),
        })
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        match inspect(sys, &self.co) {
            Found::Present => Ok(()),
            Found::Occupied(found) => bail!("occupied ({found}); not touched"),
            Found::Missing => create(sys, &self.co),
        }
    }
}

/// Whether `path` is one of the configured checkout paths.
pub fn covers(checkouts: &[Checkout], path: &Path) -> bool {
    checkouts.iter().any(|c| c.path == path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::system::Output;
    use crate::system::fake::FakeSystem;

    #[test]
    fn remotes_normalize() {
        let a = normalize_remote("https://github.com/kostadis/mneme.git");
        assert_eq!(a, "https://github.com/kostadis/mneme");
        assert_eq!(normalize_remote("https://github.com/kostadis/mneme/"), a);
        assert_eq!(normalize_remote("git@github.com:kostadis/mneme.git"), a);
        assert_eq!(normalize_remote("ssh://git@github.com/kostadis/mneme"), a);
        assert_ne!(normalize_remote("https://github.com/kostadis/campaigns"), a);
    }

    fn co() -> Checkout {
        Checkout {
            path: PathBuf::from("/home/k/src/mneme"),
            repo: "https://github.com/kostadis/mneme.git".to_string(),
            branch: "main".to_string(),
            sparse: None,
        }
    }

    #[test]
    fn inspect_outcomes() {
        let sys = FakeSystem::new("k");
        assert_eq!(inspect(&sys, &co()), Found::Missing);

        sys.set_file("/home/k/src/mneme", "x");
        assert_eq!(inspect(&sys, &co()), Found::Occupied("a file".into()));
        sys.files.borrow_mut().clear();

        sys.set_dir("/home/k/src/mneme", 1000);
        assert_eq!(
            inspect(&sys, &co()),
            Found::Occupied("not a git repository".into())
        );

        sys.set_dir("/home/k/src/mneme/.git", 1000);
        sys.respond(
            "git -C /home/k/src/mneme remote get-url origin",
            Output::ok("git@github.com:kostadis/other.git\n"),
        );
        assert_eq!(
            inspect(&sys, &co()),
            Found::Occupied("origin is git@github.com:kostadis/other.git".into())
        );

        sys.respond(
            "git -C /home/k/src/mneme remote get-url origin",
            Output::ok("git@github.com:kostadis/mneme.git\n"),
        );
        assert_eq!(inspect(&sys, &co()), Found::Present);
    }

    #[test]
    fn sparse_clone_sequence_and_cleanup_on_failure() {
        let sys = FakeSystem::new("k");
        sys.set_dir("/home/k", 1000);
        let mut c = co();
        c.path = PathBuf::from("/home/k/obelisk");
        c.repo = "https://github.com/kostadis/campaigns".to_string();
        c.sparse = Some(vec!["obelisk".into(), "scripts".into()]);
        sys.on_run(
            "git clone --no-checkout --branch main https://github.com/kostadis/campaigns /home/k/obelisk",
            |s| s.set_dir("/home/k/obelisk", 1000),
        );
        sys.respond(
            "git -C /home/k/obelisk sparse-checkout set obelisk scripts",
            Output::fail(1, "boom"),
        );
        assert!(create(&sys, &c).is_err());
        assert_eq!(
            sys.calls(),
            [
                "git clone --no-checkout --branch main https://github.com/kostadis/campaigns /home/k/obelisk",
                "git -C /home/k/obelisk sparse-checkout init --cone",
                "git -C /home/k/obelisk sparse-checkout set obelisk scripts",
                "rmdir /home/k/obelisk",
            ]
        );
    }
}
