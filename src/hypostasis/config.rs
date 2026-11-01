//! `<config-dir>/hypostasis.yaml` (contracts/hypostasis-config.md, data-model.md).
//!
//! Owned by `local-machine`; NOT hypostasis's authority file. Values whose home is the
//! authority are refused here (FR-022); shared values live in `machine.yaml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use crate::config::{self, ConfigError, display_key, expand_tilde};
use crate::machine;

pub const SUBCOMMAND: &str = "hypostasis";

pub const DEFAULT_AUTHORITY: &str = "~/.config/hypostasis/hypostasis.yaml";

/// Research R5, plus `pipx` (it installs `uv`). Not machine-specific.
pub const DEFAULT_APT: &[&str] = &[
    "git",
    "build-essential",
    "python3-dev",
    "python3-venv",
    "pipx",
    "sqlite3",
    "nodejs",
    "npm",
    "ufw",
];

const TOP_KEYS: &[&str] = &[
    "authority",
    "authority_source",
    "backup_root",
    "backups",
    "prerequisites",
    "checkouts",
    "hypostasis_source",
    "services",
    "flexai_social",
    "flexai_combat",
];
/// Top-level keys whose home is hypostasis's authority file.
const AUTHORITY_KEYS: &[&str] = &[
    "venv",
    "installer",
    "rpg_lib",
    "rpg_library_db",
    "env",
    "dgx",
    "machines",
    "data_roots",
    "components",
    "order",
    "mode",
];
const BACKUP_KINDS: &[(&str, &str)] = &[
    ("rpg_library", "rpg-lib"),
    ("flexai_social", "flexai-social"),
    ("flexai_combat", "flexai-combat"),
    ("palaces", "palaces"),
];
const ENV_KEYS: &[&str] = &["CLEF_URL", "CHAT_URL", "CLEF_MODEL", "CHAT_MODEL"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkout {
    pub path: PathBuf,
    pub repo: String,
    pub branch: String,
    pub sparse: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlexAI {
    pub port: u16,
    pub data_dir: PathBuf,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backups {
    pub rpg_library: PathBuf,
    pub flexai_social: PathBuf,
    pub flexai_combat: PathBuf,
    pub palaces: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HypostasisConfig {
    /// This file, for messages.
    pub file: PathBuf,
    pub authority: PathBuf,
    pub authority_source: PathBuf,
    pub backup_root: PathBuf,
    pub backups: Backups,
    pub apt: Vec<String>,
    pub checkouts: Vec<Checkout>,
    pub hypostasis_source: PathBuf,
    pub services_venv: PathBuf,
    pub mytools: PathBuf,
    pub flexai_social: FlexAI,
    pub flexai_combat: FlexAI,
}

pub fn load(config_dir: &Path, home: &Path) -> Result<HypostasisConfig, ConfigError> {
    let (path, mapping) = config::read_mapping(
        config_dir,
        SUBCOMMAND,
        "authority_source, backup_root, checkouts, hypostasis_source, services, flexai_social, \
         flexai_combat",
    )?;
    parse(config_dir, &path, &mapping, home)
}

struct Ctx<'a> {
    file: &'a Path,
    home: &'a Path,
}

impl Ctx<'_> {
    fn key_err(&self, key: &str, reason: impl Into<String>) -> ConfigError {
        ConfigError::InvalidKey {
            path: self.file.to_path_buf(),
            key: key.to_string(),
            reason: reason.into(),
        }
    }

    fn invalid(&self, reason: impl Into<String>) -> ConfigError {
        ConfigError::Invalid {
            path: self.file.to_path_buf(),
            reason: reason.into(),
        }
    }

    fn string(&self, key: &str, v: Option<&Value>) -> Result<Option<String>, ConfigError> {
        match v {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(self.key_err(key, "must be a string")),
        }
    }

    fn required_string(&self, key: &str, v: Option<&Value>) -> Result<String, ConfigError> {
        match self.string(key, v)? {
            Some(s) if !s.trim().is_empty() => Ok(s),
            _ => Err(self.key_err(key, "required")),
        }
    }

    fn path(&self, key: &str, v: Option<&Value>) -> Result<Option<PathBuf>, ConfigError> {
        match self.string(key, v)? {
            None => Ok(None),
            Some(s) => {
                let p = expand_tilde(s.trim(), self.home);
                if !p.is_absolute() {
                    return Err(self.key_err(key, format!("'{s}' must be an absolute path")));
                }
                Ok(Some(p))
            }
        }
    }

    fn required_path(&self, key: &str, v: Option<&Value>) -> Result<PathBuf, ConfigError> {
        self.path(key, v)?
            .ok_or_else(|| self.key_err(key, "required"))
    }

    fn mapping<'v>(
        &self,
        key: &str,
        v: Option<&'v Value>,
    ) -> Result<Option<&'v Mapping>, ConfigError> {
        match v {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Mapping(m)) => Ok(Some(m)),
            Some(_) => Err(self.key_err(key, "must be a mapping")),
        }
    }

    /// Unknown keys inside a nested mapping.
    fn only_keys(&self, key: &str, m: &Mapping, allowed: &[&str]) -> Result<(), ConfigError> {
        for k in m.keys() {
            if !allowed.contains(&k.as_str().unwrap_or("")) {
                return Err(self.key_err(key, format!("unknown key '{}'", display_key(k))));
            }
        }
        Ok(())
    }

    fn port(&self, key: &str, v: Option<&Value>) -> Result<u16, ConfigError> {
        match v {
            Some(Value::Number(n)) => match n.as_u64() {
                Some(p) if (1..=65535).contains(&p) => Ok(p as u16),
                _ => Err(self.key_err(key, format!("{n} is not a port (1-65535)"))),
            },
            None | Some(Value::Null) => Err(self.key_err(key, "required")),
            Some(_) => Err(self.key_err(key, "must be a port number (1-65535)")),
        }
    }
}

/// Any key named `password` or `token`, at any depth.
fn find_secret(v: &Value, at: &str) -> Option<String> {
    match v {
        Value::Mapping(m) => m.iter().find_map(|(k, val)| {
            let name = display_key(k);
            let here = if at.is_empty() {
                name.clone()
            } else {
                format!("{at}.{name}")
            };
            if ["password", "token"].contains(&name.to_ascii_lowercase().as_str()) {
                Some(here)
            } else {
                find_secret(val, &here)
            }
        }),
        Value::Sequence(items) => items.iter().find_map(|i| find_secret(i, at)),
        _ => None,
    }
}

pub fn parse(
    config_dir: &Path,
    file: &Path,
    mapping: &Mapping,
    home: &Path,
) -> Result<HypostasisConfig, ConfigError> {
    let c = Ctx { file, home };

    if let Some(at) = find_secret(&Value::Mapping(mapping.clone()), "") {
        return Err(c.key_err(&at, "no secrets in config (no password or token keys)"));
    }

    let authority = c
        .path("authority", mapping.get("authority"))?
        .unwrap_or_else(|| expand_tilde(DEFAULT_AUTHORITY, home));

    for key in mapping.keys() {
        let name = key.as_str().unwrap_or("");
        if name == "allowed_network" || name == "owner" {
            return Err(c.key_err(
                name,
                format!("moved to {}", machine::path(config_dir).display()),
            ));
        }
        if AUTHORITY_KEYS.contains(&name) {
            return Err(c.key_err(
                name,
                format!(
                    "lives in {} (hypostasis's authority file), not here",
                    authority.display()
                ),
            ));
        }
        if !TOP_KEYS.contains(&name) {
            return Err(c.invalid(format!("unknown key '{}'", display_key(key))));
        }
    }

    let authority_source = c.required_path("authority_source", mapping.get("authority_source"))?;
    let backup_root = c.required_path("backup_root", mapping.get("backup_root"))?;

    let backups_map = c.mapping("backups", mapping.get("backups"))?;
    if let Some(m) = backups_map {
        let kinds: Vec<&str> = BACKUP_KINDS.iter().map(|(k, _)| *k).collect();
        c.only_keys("backups", m, &kinds)?;
    }
    let backup = |kind: &str, folder: &str| -> Result<PathBuf, ConfigError> {
        let v = backups_map.and_then(|m| m.get(kind));
        Ok(c.path(&format!("backups.{kind}"), v)?
            .unwrap_or_else(|| backup_root.join(folder)))
    };
    let backups = Backups {
        rpg_library: backup("rpg_library", "rpg-lib")?,
        flexai_social: backup("flexai_social", "flexai-social")?,
        flexai_combat: backup("flexai_combat", "flexai-combat")?,
        palaces: backup("palaces", "palaces")?,
    };

    let apt = match c.mapping("prerequisites", mapping.get("prerequisites"))? {
        None => DEFAULT_APT.iter().map(|s| s.to_string()).collect(),
        Some(m) => {
            c.only_keys("prerequisites", m, &["apt"])?;
            match m.get("apt") {
                None => DEFAULT_APT.iter().map(|s| s.to_string()).collect(),
                Some(Value::Sequence(items)) => {
                    let mut pkgs = Vec::new();
                    for i in items {
                        match i.as_str() {
                            Some(p)
                                if !p.is_empty()
                                    && p.chars().all(|ch| {
                                        ch.is_ascii_alphanumeric() || "+-.".contains(ch)
                                    }) =>
                            {
                                pkgs.push(p.to_string())
                            }
                            _ => {
                                return Err(c.key_err(
                                    "prerequisites.apt",
                                    format!("'{}' is not a package name", display_key(i)),
                                ));
                            }
                        }
                    }
                    pkgs
                }
                Some(_) => {
                    return Err(c.key_err("prerequisites.apt", "must be a list of package names"));
                }
            }
        }
    };

    let checkouts = parse_checkouts(&c, mapping.get("checkouts"))?;
    let is_checkout = |p: &Path| checkouts.iter().any(|ch| ch.path == p);

    let hypostasis_source =
        c.required_path("hypostasis_source", mapping.get("hypostasis_source"))?;
    if !is_checkout(&hypostasis_source) {
        return Err(c.key_err(
            "hypostasis_source",
            format!(
                "'{}' must be one of the checkout paths",
                hypostasis_source.display()
            ),
        ));
    }

    let services = c
        .mapping("services", mapping.get("services"))?
        .ok_or_else(|| c.key_err("services", "required (venv, mytools)"))?;
    if services.contains_key("rpg_lib") {
        return Err(c.key_err(
            "services.rpg_lib",
            format!(
                "lives in {} (hypostasis's authority file), not here",
                authority.display()
            ),
        ));
    }
    c.only_keys("services", services, &["venv", "mytools"])?;
    let services_venv = c.required_path("services.venv", services.get("venv"))?;
    let mytools = c.required_path("services.mytools", services.get("mytools"))?;
    if !is_checkout(&mytools) {
        return Err(c.key_err(
            "services.mytools",
            format!("'{}' must be one of the checkout paths", mytools.display()),
        ));
    }

    let flexai_social = parse_flexai(&c, "flexai_social", mapping.get("flexai_social"), true)?;
    let flexai_combat = parse_flexai(&c, "flexai_combat", mapping.get("flexai_combat"), false)?;
    if flexai_social.port == flexai_combat.port {
        return Err(c.key_err(
            "flexai_combat.port",
            format!("{} is already flexai_social's port", flexai_combat.port),
        ));
    }

    Ok(HypostasisConfig {
        file: file.to_path_buf(),
        authority,
        authority_source,
        backup_root,
        backups,
        apt,
        checkouts,
        hypostasis_source,
        services_venv,
        mytools,
        flexai_social,
        flexai_combat,
    })
}

fn parse_checkouts(c: &Ctx, v: Option<&Value>) -> Result<Vec<Checkout>, ConfigError> {
    let items = match v {
        Some(Value::Sequence(items)) if !items.is_empty() => items,
        None | Some(Value::Null) => return Err(c.key_err("checkouts", "required")),
        Some(Value::Sequence(_)) => return Err(c.key_err("checkouts", "must not be empty")),
        Some(_) => return Err(c.key_err("checkouts", "must be a list")),
    };
    let mut out: Vec<Checkout> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let at = format!("checkouts[{i}]");
        let m = c
            .mapping(&at, Some(item))?
            .ok_or_else(|| c.key_err(&at, "must be a mapping"))?;
        c.only_keys(&at, m, &["path", "repo", "branch", "sparse"])?;
        let path = c.required_path(&format!("{at}.path"), m.get("path"))?;
        let repo = c.required_string(&format!("{at}.repo"), m.get("repo"))?;
        let branch = c.required_string(&format!("{at}.branch"), m.get("branch"))?;
        let sparse = match m.get("sparse") {
            None | Some(Value::Null) => None,
            Some(Value::Sequence(dirs)) if !dirs.is_empty() => {
                let mut names = Vec::new();
                for d in dirs {
                    match d.as_str() {
                        Some(n) if !n.is_empty() && !n.contains('/') && n != "." && n != ".." => {
                            names.push(n.to_string())
                        }
                        _ => {
                            return Err(c.key_err(
                                &format!("{at}.sparse"),
                                format!("'{}' must be a plain directory name", display_key(d)),
                            ));
                        }
                    }
                }
                Some(names)
            }
            Some(_) => {
                return Err(c.key_err(
                    &format!("{at}.sparse"),
                    "must be a non-empty list of directory names",
                ));
            }
        };
        for other in &out {
            if other.path == path {
                return Err(c.key_err(
                    &format!("{at}.path"),
                    format!("'{}' is listed twice", path.display()),
                ));
            }
            if path.starts_with(&other.path) || other.path.starts_with(&path) {
                return Err(c.key_err(
                    &format!("{at}.path"),
                    format!(
                        "'{}' and '{}' are nested; checkouts must not be inside each other",
                        path.display(),
                        other.path.display()
                    ),
                ));
            }
        }
        out.push(Checkout {
            path,
            repo,
            branch,
            sparse,
        });
    }
    Ok(out)
}

fn parse_flexai(
    c: &Ctx,
    key: &str,
    v: Option<&Value>,
    env_allowed: bool,
) -> Result<FlexAI, ConfigError> {
    let m = c
        .mapping(key, v)?
        .ok_or_else(|| c.key_err(key, "required (port, data_dir)"))?;
    let allowed: &[&str] = if env_allowed {
        &["port", "data_dir", "env"]
    } else {
        &["port", "data_dir"]
    };
    if !env_allowed && m.contains_key("env") {
        return Err(c.key_err(&format!("{key}.env"), "only flexai_social takes env"));
    }
    c.only_keys(key, m, allowed)?;
    let port = c.port(&format!("{key}.port"), m.get("port"))?;
    let data_dir = c.required_path(&format!("{key}.data_dir"), m.get("data_dir"))?;
    let mut env = BTreeMap::new();
    if let Some(e) = c.mapping(&format!("{key}.env"), m.get("env"))? {
        for (k, val) in e {
            let name = display_key(k);
            if !ENV_KEYS.contains(&name.as_str()) {
                return Err(c.key_err(
                    &format!("{key}.env"),
                    format!("'{name}' is not one of {}", ENV_KEYS.join(", ")),
                ));
            }
            let value = c
                .string(&format!("{key}.env.{name}"), Some(val))?
                .ok_or_else(|| c.key_err(&format!("{key}.env.{name}"), "must have a value"))?;
            env.insert(name, value);
        }
    }
    Ok(FlexAI {
        port,
        data_dir,
        env,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/kostadis";
    const FILE: &str = "/cfg/hypostasis.yaml";

    pub const EXAMPLE: &str = r#"
authority: ~/.config/hypostasis/hypostasis.yaml
authority_source: ~/src/campaigns-private/hypostasis.yaml
backup_root: "/mnt/g/My Drive/hypostasis/backups"
backups:
  palaces: /srv/palaces
prerequisites:
  apt: [git, build-essential, python3-dev, python3-venv, pipx, sqlite3, nodejs, npm, ufw]
checkouts:
  - { path: ~/src/mneme,   repo: https://github.com/kostadis/mneme.git,   branch: main }
  - { path: ~/src/mytools, repo: https://github.com/kostadis/mytools.git, branch: main }
  - { path: ~/src/campaigns-private, repo: https://github.com/kostadis/campaigns-private.git, branch: main }
  - path: ~/obelisk
    repo: https://github.com/kostadis/campaigns
    branch: main
    sparse: [obelisk, gm-assistant, scripts, tools, .claude]
hypostasis_source: ~/src/mneme
services:
  venv: ~/.venvs/services
  mytools: ~/src/mytools
flexai_social:
  port: 5105
  data_dir: ~/data/flexai/social
  env:
    CLEF_URL: http://192.168.1.121:8002/v1
    CHAT_URL: http://192.168.1.121:8003/v1
flexai_combat:
  port: 5106
  data_dir: ~/data/flexai/combat
"#;

    fn parse_str(text: &str) -> Result<HypostasisConfig, ConfigError> {
        let file = Path::new(FILE);
        let mapping = config::parse_mapping(file, text)?;
        parse(Path::new("/cfg"), file, &mapping, Path::new(HOME))
    }

    /// The example with `old` replaced by `new` (which must occur).
    fn variant(old: &str, new: &str) -> String {
        assert!(EXAMPLE.contains(old), "example lacks {old:?}");
        EXAMPLE.replacen(old, new, 1)
    }

    fn err(text: &str) -> String {
        parse_str(text).unwrap_err().to_string()
    }

    #[test]
    fn full_example_parses() {
        let cfg = parse_str(EXAMPLE).unwrap();
        assert_eq!(
            cfg.authority,
            PathBuf::from("/home/kostadis/.config/hypostasis/hypostasis.yaml")
        );
        assert_eq!(
            cfg.backups.rpg_library,
            PathBuf::from("/mnt/g/My Drive/hypostasis/backups/rpg-lib")
        );
        assert_eq!(cfg.backups.palaces, PathBuf::from("/srv/palaces"));
        assert_eq!(cfg.checkouts.len(), 4);
        assert_eq!(
            cfg.checkouts[3].sparse.as_deref().unwrap(),
            ["obelisk", "gm-assistant", "scripts", "tools", ".claude"]
        );
        assert_eq!(cfg.mytools, PathBuf::from("/home/kostadis/src/mytools"));
        assert_eq!(
            cfg.flexai_social.env["CLEF_URL"],
            "http://192.168.1.121:8002/v1"
        );
        assert!(cfg.flexai_combat.env.is_empty());
    }

    #[test]
    fn defaults() {
        let text = variant(
            "authority: ~/.config/hypostasis/hypostasis.yaml\n",
            "",
        )
        .replace("backups:\n  palaces: /srv/palaces\n", "")
        .replace(
            "prerequisites:\n  apt: [git, build-essential, python3-dev, python3-venv, pipx, sqlite3, nodejs, npm, ufw]\n",
            "",
        );
        let cfg = parse_str(&text).unwrap();
        assert_eq!(
            cfg.authority,
            PathBuf::from("/home/kostadis/.config/hypostasis/hypostasis.yaml")
        );
        assert_eq!(
            cfg.backups.palaces,
            PathBuf::from("/mnt/g/My Drive/hypostasis/backups/palaces")
        );
        assert_eq!(cfg.apt, DEFAULT_APT);
    }

    #[test]
    fn unknown_key() {
        assert_eq!(
            err(&format!("{EXAMPLE}\nbackup_rot: /x\n")),
            format!("{FILE}: unknown key 'backup_rot'")
        );
    }

    #[test]
    fn shared_keys_moved_to_machine_yaml() {
        assert_eq!(
            err(&format!("{EXAMPLE}\nallowed_network: 10.0.0.0/8\n")),
            format!("{FILE}: allowed_network: moved to /cfg/machine.yaml")
        );
        assert_eq!(
            err(&format!("{EXAMPLE}\nowner: kostadis\n")),
            format!("{FILE}: owner: moved to /cfg/machine.yaml")
        );
    }

    #[test]
    fn authority_keys_are_refused() {
        for key in [
            "venv: ~/.venvs/main",
            "installer: uv",
            "rpg_library_db: /x",
            "env: {}",
            "dgx: {}",
        ] {
            let e = err(&format!("{EXAMPLE}\n{key}\n"));
            assert!(
                e.contains("lives in /home/kostadis/.config/hypostasis/hypostasis.yaml"),
                "{key}: {e}"
            );
        }
        let e = err(&variant(
            "  venv: ~/.venvs/services\n",
            "  venv: ~/.venvs/services\n  rpg_lib: {port: 8000}\n",
        ));
        assert!(e.contains("services.rpg_lib: lives in"), "{e}");
    }

    #[test]
    fn required_keys() {
        for line in [
            "authority_source: ~/src/campaigns-private/hypostasis.yaml\n",
            "backup_root: \"/mnt/g/My Drive/hypostasis/backups\"\n",
            "hypostasis_source: ~/src/mneme\n",
        ] {
            let key = line.split(':').next().unwrap();
            assert_eq!(err(&variant(line, "")), format!("{FILE}: {key}: required"));
        }
    }

    #[test]
    fn relative_path_is_rejected() {
        assert_eq!(
            err(&variant(
                "backup_root: \"/mnt/g/My Drive/hypostasis/backups\"",
                "backup_root: backups"
            )),
            format!("{FILE}: backup_root: 'backups' must be an absolute path")
        );
    }

    #[test]
    fn duplicate_and_nested_checkouts() {
        let dup = variant(
            "  - { path: ~/src/mytools,",
            "  - { path: ~/src/mneme, repo: x, branch: main }\n  - { path: ~/src/mytools,",
        );
        assert!(err(&dup).contains("is listed twice"));
        let nested = variant(
            "  - { path: ~/src/mytools,",
            "  - { path: ~/src/mneme/sub, repo: x, branch: main }\n  - { path: ~/src/mytools,",
        );
        assert!(err(&nested).contains("are nested"));
    }

    #[test]
    fn checkout_fields() {
        let e = err(&variant("branch: main }", "branch: \"\" }"));
        assert_eq!(e, format!("{FILE}: checkouts[0].branch: required"));
        let e = err(&variant(
            "sparse: [obelisk, gm-assistant, scripts, tools, .claude]",
            "sparse: []",
        ));
        assert!(
            e.contains("checkouts[3].sparse: must be a non-empty list"),
            "{e}"
        );
        let e = err(&variant("sparse: [obelisk,", "sparse: [a/b,"));
        assert!(e.contains("must be a plain directory name"), "{e}");
    }

    #[test]
    fn mytools_and_hypostasis_source_must_be_checkouts() {
        let e = err(&variant(
            "  mytools: ~/src/mytools",
            "  mytools: ~/elsewhere",
        ));
        assert!(
            e.contains(
                "services.mytools: '/home/kostadis/elsewhere' must be one of the checkout paths"
            ),
            "{e}"
        );
        let e = err(&variant(
            "hypostasis_source: ~/src/mneme",
            "hypostasis_source: ~/src/hyp",
        ));
        assert!(
            e.contains(
                "hypostasis_source: '/home/kostadis/src/hyp' must be one of the checkout paths"
            ),
            "{e}"
        );
    }

    #[test]
    fn ports() {
        let e = err(&variant("port: 5106", "port: 5105"));
        assert!(
            e.contains("flexai_combat.port: 5105 is already flexai_social's port"),
            "{e}"
        );
        let e = err(&variant("port: 5106", "port: 70000"));
        assert!(e.contains("70000 is not a port"), "{e}");
    }

    #[test]
    fn env_rules() {
        let e = err(&variant("    CHAT_URL:", "    API_KEY: x\n    CHAT_URL:"));
        assert!(e.contains("'API_KEY' is not one of"), "{e}");
        let e = err(&variant(
            "  data_dir: ~/data/flexai/combat",
            "  data_dir: ~/data/flexai/combat\n  env: {CLEF_URL: x}",
        ));
        assert!(
            e.contains("flexai_combat.env: only flexai_social takes env"),
            "{e}"
        );
    }

    #[test]
    fn secrets_are_refused() {
        let e = err(&variant(
            "  venv: ~/.venvs/services",
            "  venv: ~/.venvs/services\n  token: abc",
        ));
        assert!(e.contains("services.token: no secrets in config"), "{e}");
    }

    #[test]
    fn backup_kinds_are_limited() {
        let e = err(&variant("  palaces: /srv/palaces", "  photos: /srv/x"));
        assert!(e.contains("backups: unknown key 'photos'"), "{e}");
    }
}
