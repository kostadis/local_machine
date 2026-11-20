//! The library palace as local-machine sees it: state from `library status --json`, the
//! live-palace guard, and the embedder probe (specs/003-dmsguild-library-palace research R1, R4, R7).

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};

use super::Context;
use super::authority::{Authority, LibraryKeys};
use crate::say;
use crate::system::action::{Check, Step};
use crate::system::{Cmd, PathKind, System};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryState {
    Ready {
        books: u64,
        stale: usize,
        index_bytes: u64,
    },
    NotConfigured(String),
    NotBuilt(String),
    EmbedderMismatch(String),
    LivePalace(String),
    Failed(String),
}

impl LibraryState {
    pub fn exit_code(&self) -> u8 {
        match self {
            LibraryState::Ready { .. } => 0,
            LibraryState::NotConfigured(_) => 3,
            LibraryState::NotBuilt(_) => 4,
            _ => 1,
        }
    }
}

/// `<venv>/bin/library <sub...>` with the authority's environment.
pub fn library_cmd(a: &Authority, sub: &[&str]) -> Cmd {
    Cmd::new(&a.bin("library").to_string_lossy())
        .args(sub)
        .envs(&a.env)
}

pub fn observe(sys: &dyn System, a: &Authority) -> LibraryState {
    if sys.path_kind(&a.bin("library")) == PathKind::Missing {
        return LibraryState::Failed(
            "needs the library command from CampaignGenerator".to_string(),
        );
    }
    let out = match sys.run(&library_cmd(a, &["status", "--json"])) {
        Ok(o) => o,
        Err(e) => return LibraryState::Failed(format!("library status --json: {e}")),
    };
    let message = || {
        let err = out.stderr.trim();
        if err.is_empty() {
            out.stdout.trim().to_string()
        } else {
            err.to_string()
        }
    };
    match out.code {
        0 => match serde_json::from_str::<serde_json::Value>(&out.stdout) {
            Ok(v) => LibraryState::Ready {
                books: v.get("books").and_then(|b| b.as_u64()).unwrap_or(0),
                stale: v
                    .get("stale")
                    .and_then(|s| s.as_array())
                    .map_or(0, |s| s.len()),
                index_bytes: v.get("index_bytes").and_then(|b| b.as_u64()).unwrap_or(0),
            },
            Err(e) => LibraryState::Failed(format!("library status --json: {e}")),
        },
        3 => LibraryState::NotConfigured(message()),
        4 => LibraryState::NotBuilt(message()),
        5 => LibraryState::EmbedderMismatch(message()),
        6 => LibraryState::LivePalace(message()),
        code => {
            let m = message();
            LibraryState::Failed(if m.is_empty() {
                format!("library status --json exited {code}")
            } else {
                m
            })
        }
    }
}

/// Resolve links; for a path that does not exist yet, resolve its nearest existing ancestor
/// and re-join the rest.
fn resolve(sys: &dyn System, path: &Path) -> PathBuf {
    let mut rest: Vec<Component> = Vec::new();
    let mut cur = path;
    loop {
        if let Some(c) = sys.canonical(cur) {
            let mut out = c;
            for comp in rest.iter().rev() {
                out.push(comp.as_os_str());
            }
            return out;
        }
        match (cur.parent(), cur.components().next_back()) {
            (Some(parent), Some(last)) => {
                rest.push(last);
                cur = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Refuse a palace that equals, sits inside, or contains a live palace or campaign tree.
pub fn guard(
    sys: &dyn System,
    ctx: &Context,
    a: &Authority,
    keys: &LibraryKeys,
) -> Result<(), String> {
    let palace = resolve(sys, &keys.palace);
    let mut roots = vec![
        a.mempalace_root.clone(),
        ctx.home().join("src").join("campaigns"),
    ];
    roots.extend(a.campaigns.iter().cloned());
    for root in roots {
        let r = resolve(sys, &root);
        if palace.starts_with(&r) || r.starts_with(&palace) {
            return Err(format!(
                "{} overlaps live palace {}",
                keys.palace.display(),
                root.display()
            ));
        }
    }
    Ok(())
}

/// Where the embedder lives, parsed from `MEMPALACE_EMBEDDING_ENDPOINT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Embedder {
    pub url: String,
    pub host: String,
    pub port: u16,
    pub path: &'static str,
}

/// `None` when no endpoint is set, or it is not plain `http://` (unknown, cannot probe).
pub fn embedder(a: &Authority) -> Option<Embedder> {
    let get = |k: &str| a.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let url = get("MEMPALACE_EMBEDDING_ENDPOINT")?.trim().to_string();
    let rest = url.strip_prefix("http://")?;
    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().ok()?),
        None => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return None;
    }
    let path = if get("MEMPALACE_EMBEDDING_PROVIDER") == Some("ollama") {
        "/api/tags"
    } else {
        "/"
    };
    Some(Embedder {
        url,
        host,
        port,
        path,
    })
}

/// Does the embedder answer? `None` = cannot probe. Ollama must answer 2xx; others any status.
pub fn embedder_answers(sys: &dyn System, a: &Authority) -> Option<bool> {
    let e = embedder(a)?;
    let status = sys.http_status_at(&e.host, e.port, e.path, Duration::from_secs(3));
    Some(match status {
        Some(s) if e.path == "/api/tags" => (200..300).contains(&s),
        Some(_) => true,
        None => false,
    })
}

const SETUP: &str = "local-machine hypostasis setup";

/// The `library palace` setup piece: build the DMG library palace when it is missing, and
/// remove a half-built one if the build fails (specs/003-dmsguild-library-palace R5, R6, R10).
pub struct LibraryPalace<'a> {
    ctx: &'a Context,
    /// Set only once the guard has passed and the build is about to run.
    created: bool,
}

impl<'a> LibraryPalace<'a> {
    pub fn new(ctx: &'a Context) -> Self {
        LibraryPalace {
            ctx,
            created: false,
        }
    }

    /// P1..P5 in order; the first failure is `<reason> — <fix>`.
    fn preconditions(&self, sys: &dyn System, a: &Authority) -> Result<LibraryKeys, String> {
        let keys = match &a.library {
            Ok(k) => k.clone(),
            Err(reason) => {
                return Err(format!(
                    "not configured ({reason} in {}) — add it there and to {}",
                    self.ctx.cfg.authority.display(),
                    self.ctx.cfg.authority_source.display()
                ));
            }
        };
        if sys.path_kind(&a.bin("library")) == PathKind::Missing {
            return Err(format!(
                "needs the library command from CampaignGenerator — run '{SETUP}'"
            ));
        }
        if sys.path_kind(&a.rpg_library_db) == PathKind::Missing {
            return Err(format!(
                "needs the rpg library DB — run '{SETUP}' (restores it)"
            ));
        }
        if sys.path_kind(&keys.source_root) != PathKind::Dir {
            let p = keys.source_root.display();
            return Err(format!(
                "source folder {p} not readable — mount {p}, then run '{SETUP}'"
            ));
        }
        guard(sys, self.ctx, a, &keys).map_err(|e| {
            format!(
                "{e} — change library.palace in {}",
                self.ctx.cfg.authority.display()
            )
        })?;
        Ok(keys)
    }
}

impl Step for LibraryPalace<'_> {
    fn name(&self) -> &str {
        "library palace"
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let Some(a) = self.ctx.authority(sys)? else {
            return Ok(Check::NeedsChange("needs the authority file".into()));
        };
        if a.library.is_err() || sys.path_kind(&a.bin("library")) == PathKind::Missing {
            return Ok(Check::NeedsChange(match self.preconditions(sys, &a) {
                Err(e) => e,
                Ok(_) => "needs the library command".into(),
            }));
        }
        Ok(match observe(sys, &a) {
            LibraryState::Ready { books, .. } => Check::DoneWith(format!("{books} books")),
            LibraryState::EmbedderMismatch(_) | LibraryState::LivePalace(_) => {
                Check::DoneWith("present; see 'local-machine hypostasis status'".into())
            }
            LibraryState::NotBuilt(_) => Check::NeedsChange(match self.preconditions(sys, &a) {
                Err(e) => e,
                Ok(_) => "not built".into(),
            }),
            LibraryState::NotConfigured(m) | LibraryState::Failed(m) => Check::NeedsChange(m),
        })
    }

    fn notice(&self, _found: &Check) -> Option<String> {
        None
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(a) = self.ctx.authority(sys)? else {
            bail!("{} does not exist", self.ctx.cfg.authority.display());
        };
        let keys = match self.preconditions(sys, &a) {
            Ok(k) => k,
            Err(e) => bail!("{e}"),
        };
        if sys.path_kind(&keys.palace) != PathKind::Missing {
            bail!(
                "{} occupied (not a built library); not touched",
                keys.palace.display()
            );
        }
        if let Some(e) = embedder(&a) {
            if embedder_answers(sys, &a) == Some(false) {
                bail!(
                    "embedder {} not answering — start it, then run '{SETUP}'",
                    e.url
                );
            }
        } else if let Some(url) = a
            .env
            .iter()
            .find(|(k, _)| k == "MEMPALACE_EMBEDDING_ENDPOINT")
            .map(|(_, v)| v)
        {
            say!("unknown: embedder {url} (cannot probe https); building anyway");
        }
        say!(
            "note: library palace: building from {}; this can take a while",
            keys.source_root.display()
        );
        // Only now may a failure remove the palace path: it did not exist, and the guard passed.
        self.created = true;
        let code = sys
            .run(&library_cmd(&a, &["build"]).interactive())
            .map(|o| o.code);
        match code {
            Ok(0) => {}
            Ok(n) => {
                self.remove_partial(sys, &keys);
                bail!("build failed (exit {n}); removed the partial palace");
            }
            Err(e) => {
                self.remove_partial(sys, &keys);
                bail!("build failed ({e}); removed the partial palace");
            }
        }
        match observe(sys, &a) {
            LibraryState::Ready { books, .. } => {
                say!("note: library palace: {books} books");
                Ok(())
            }
            other => {
                let msg = match other {
                    LibraryState::NotConfigured(m)
                    | LibraryState::NotBuilt(m)
                    | LibraryState::EmbedderMismatch(m)
                    | LibraryState::LivePalace(m)
                    | LibraryState::Failed(m) => m,
                    LibraryState::Ready { .. } => String::new(),
                };
                self.remove_partial(sys, &keys);
                bail!("built but library status says: {msg}; removed the partial palace");
            }
        }
    }

    fn rollback(&mut self, sys: &dyn System) -> Result<()> {
        if self.created
            && let Some(a) = self.ctx.authority(sys)?
            && let Ok(keys) = &a.library
            && guard(sys, self.ctx, &a, keys).is_ok()
        {
            self.remove_partial(sys, keys);
        }
        Ok(())
    }

    fn continue_on_failure(&self) -> bool {
        true
    }
}

impl LibraryPalace<'_> {
    /// Remove `keys.palace` and nothing else; callers reach this only after the guard passed
    /// and only when the path did not exist before this apply (`created`).
    fn remove_partial(&mut self, sys: &dyn System, keys: &LibraryKeys) {
        if self.created && sys.path_kind(&keys.palace) != PathKind::Missing {
            let _ = sys.remove_dir_all(&keys.palace);
        }
        self.created = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hypostasis::test_support::{authority, context};
    use crate::system::Output;
    use crate::system::fake::FakeSystem;

    const LIB: &str = "/home/kostadis/.venvs/main/bin/library";

    fn sys_with_lib() -> FakeSystem {
        let s = FakeSystem::new("kostadis");
        s.set_file(LIB, "");
        s
    }

    fn observe_with(out: Output) -> LibraryState {
        let s = sys_with_lib();
        s.respond(&format!("{LIB} status --json"), out);
        observe(&s, &authority())
    }

    #[test]
    fn exit_codes_map_to_states() {
        let cases = [
            (3, "NotConfigured", 3),
            (4, "NotBuilt", 4),
            (5, "EmbedderMismatch", 1),
            (6, "LivePalace", 1),
            (2, "Failed", 1),
        ];
        for (code, name, exit) in cases {
            let st = observe_with(Output::fail(code, " boom \n"));
            assert!(format!("{st:?}").starts_with(name), "{code}: {st:?}");
            assert_eq!(st.exit_code(), exit);
            assert!(format!("{st:?}").contains("\"boom\""), "{st:?}");
        }
    }

    #[test]
    fn message_falls_back_to_stdout() {
        let st = observe_with(Output {
            code: 4,
            stdout: "from stdout".into(),
            stderr: String::new(),
        });
        assert_eq!(st, LibraryState::NotBuilt("from stdout".into()));
    }

    #[test]
    fn ready_parses_json() {
        let st = observe_with(Output::ok(
            r#"{"books": 2, "stale": ["a","b"], "index_bytes": 99, "embedder_match": true}"#,
        ));
        assert_eq!(
            st,
            LibraryState::Ready {
                books: 2,
                stale: 2,
                index_bytes: 99
            }
        );
        assert_eq!(st.exit_code(), 0);
    }

    #[test]
    fn bad_json_is_failed() {
        let st = observe_with(Output::ok("not json"));
        match st {
            LibraryState::Failed(m) => assert!(m.starts_with("library status --json: "), "{m}"),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn missing_binary_is_not_run() {
        let s = FakeSystem::new("kostadis");
        let st = observe(&s, &authority());
        assert_eq!(
            st,
            LibraryState::Failed("needs the library command from CampaignGenerator".into())
        );
        assert!(s.calls().iter().all(|c| !c.contains("library status")));
    }

    #[test]
    fn observe_runs_with_the_authority_env() {
        let s = sys_with_lib();
        s.respond(
            &format!("{LIB} status --json"),
            Output::ok(r#"{"books":1}"#),
        );
        let a = authority();
        observe(&s, &a);
        assert_eq!(s.env_of(&format!("{LIB} status")), Some(a.env.clone()));
    }

    fn guard_for(palace: &str, s: &FakeSystem) -> Result<(), String> {
        let a = authority();
        let keys = LibraryKeys {
            palace: palace.into(),
            source_root: "/src".into(),
        };
        guard(s, &context(), &a, &keys)
    }

    #[test]
    fn guard_refuses_overlaps() {
        let s = FakeSystem::new("kostadis");
        let e = guard_for("/home/kostadis/.mempalace/palaces/x", &s).unwrap_err();
        assert_eq!(
            e,
            "/home/kostadis/.mempalace/palaces/x overlaps live palace /home/kostadis/.mempalace"
        );
        assert!(guard_for("/home/kostadis/phandalin", &s).is_err());
        assert!(guard_for("/home/kostadis/phandalin/sub/deep", &s).is_err());
        assert!(guard_for("/home/kostadis/src", &s).is_err()); // contains ~/src/campaigns
        assert!(guard_for("/home/kostadis/src/campaigns", &s).is_err());
    }

    #[test]
    fn guard_follows_links() {
        let s = FakeSystem::new("kostadis");
        s.set_dir("/home/kostadis/.local/share/hypostasis/library", 1000);
        s.set_canonical(
            "/home/kostadis/.local/share/hypostasis/library",
            "/home/kostadis/.mempalace/real",
        );
        assert!(guard_for("/home/kostadis/.local/share/hypostasis/library/palace", &s).is_err());
    }

    #[test]
    fn guard_passes_the_default_palace() {
        let s = FakeSystem::new("kostadis");
        assert_eq!(
            guard_for("/home/kostadis/.local/share/hypostasis/library/palace", &s),
            Ok(())
        );
    }

    fn with_env(pairs: &[(&str, &str)]) -> Authority {
        let mut a = authority();
        a.env = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        a
    }

    #[test]
    fn embedder_parses_the_endpoint() {
        let e = embedder(&authority()).unwrap();
        assert_eq!(e.url, "http://192.168.56.1:11434");
        assert_eq!(e.host, "192.168.56.1");
        assert_eq!(e.port, 11434);
        assert_eq!(e.path, "/api/tags");
        let e = embedder(&with_env(&[(
            "MEMPALACE_EMBEDDING_ENDPOINT",
            "http://h/v1",
        )]))
        .unwrap();
        assert_eq!((e.host.as_str(), e.port, e.path), ("h", 80, "/"));
        assert!(embedder(&with_env(&[("MEMPALACE_EMBEDDING_ENDPOINT", "https://h")])).is_none());
        assert!(embedder(&with_env(&[])).is_none());
    }

    #[test]
    fn embedder_probe() {
        let a = authority();
        let s = FakeSystem::new("kostadis");
        s.set_http_at("192.168.56.1", 11434, &[Some(200)]);
        assert_eq!(embedder_answers(&s, &a), Some(true));
        s.set_http_at("192.168.56.1", 11434, &[Some(404)]);
        assert_eq!(embedder_answers(&s, &a), Some(false));
        s.set_http_at("192.168.56.1", 11434, &[None]);
        assert_eq!(embedder_answers(&s, &a), Some(false));
        assert_eq!(
            embedder_answers(
                &s,
                &with_env(&[("MEMPALACE_EMBEDDING_ENDPOINT", "https://h")])
            ),
            None
        );
        let other = with_env(&[("MEMPALACE_EMBEDDING_ENDPOINT", "http://h:9")]);
        s.set_http_at("h", 9, &[Some(404)]);
        assert_eq!(embedder_answers(&s, &other), Some(true));
    }
}
