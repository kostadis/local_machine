//! `<config-dir>/export.yaml` (contracts/export-config.md, data-model.md → ExportConfig).

use std::path::{Path, PathBuf};
use std::str::FromStr;

use ipnet::IpNet;
use serde_yaml_ng::{Mapping, Value};

use crate::config::{self, ConfigError};
use crate::system::{Cmd, System};

pub const SUBCOMMAND: &str = "export";
const KEYS: &[&str] = &["allowed_network", "share_name", "path", "owner", "exclude"];
const DEFAULT_SHARE_NAME: &str = "home";
const DEFAULT_EXCLUDE: &[&str] = &[".ssh", ".gnupg"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportConfig {
    pub allowed_network: IpNet,
    pub share_name: String,
    pub path: PathBuf,
    pub owner: String,
    pub exclude: Vec<String>,
}

/// The file as written, before defaults that need the system (owner, path).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawConfig {
    pub allowed_network: IpNet,
    pub share_name: String,
    pub path: Option<PathBuf>,
    pub owner: Option<String>,
    pub exclude: Vec<String>,
}

pub fn load(config_dir: &Path, sys: &dyn System) -> Result<ExportConfig, ConfigError> {
    let (path, mapping) = config::read_mapping(config_dir, SUBCOMMAND, "allowed_network")?;
    let raw = parse(&path, &mapping)?;
    resolve(&path, raw, sys)
}

pub fn parse(path: &Path, mapping: &Mapping) -> Result<RawConfig, ConfigError> {
    let key_err = |key: &str, reason: String| ConfigError::InvalidKey {
        path: path.to_path_buf(),
        key: key.to_string(),
        reason,
    };

    for key in mapping.keys() {
        let name = key.as_str().unwrap_or("");
        if name == "password" {
            return Err(ConfigError::Invalid {
                path: path.to_path_buf(),
                reason: "passwords are not stored in config; run 'local-machine export setup'"
                    .to_string(),
            });
        }
        if !KEYS.contains(&name) {
            return Err(ConfigError::Invalid {
                path: path.to_path_buf(),
                reason: format!("unknown key '{}'", display_key(key)),
            });
        }
    }

    let string = |key: &str| -> Result<Option<String>, ConfigError> {
        match mapping.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(key_err(key, "must be a string".to_string())),
        }
    };

    let allowed_network = match string("allowed_network")? {
        None => return Err(key_err("allowed_network", "required".to_string())),
        Some(s) => IpNet::from_str(s.trim())
            .map_err(|_| {
                key_err(
                    "allowed_network",
                    format!("'{s}' is not a network like 192.168.1.0/24"),
                )
            })?
            .trunc(),
    };

    let share_name = string("share_name")?.unwrap_or_else(|| DEFAULT_SHARE_NAME.to_string());
    if share_name.is_empty()
        || share_name.len() > 80
        || !share_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(key_err(
            "share_name",
            format!("'{share_name}' must be 1-80 letters, digits, '_' or '-'"),
        ));
    }

    let path_value = string("path")?.map(PathBuf::from);
    if let Some(p) = &path_value
        && !p.is_absolute()
    {
        return Err(key_err(
            "path",
            format!("'{}' must be an absolute path", p.display()),
        ));
    }

    let owner = string("owner")?;

    let exclude = match mapping.get("exclude") {
        None => DEFAULT_EXCLUDE.iter().map(|s| s.to_string()).collect(),
        Some(Value::Null) => Vec::new(),
        Some(Value::Sequence(items)) => {
            let mut names = Vec::new();
            for item in items {
                let name = item
                    .as_str()
                    .ok_or_else(|| key_err("exclude", "entries must be strings".to_string()))?;
                if name.contains('/') {
                    return Err(key_err(
                        "exclude",
                        format!("'{name}' must be a name, not a path"),
                    ));
                }
                if name.is_empty() || name == "." || name == ".." {
                    return Err(key_err("exclude", format!("'{name}' is not a valid name")));
                }
                names.push(name.to_string());
            }
            names
        }
        Some(_) => return Err(key_err("exclude", "must be a list of names".to_string())),
    };

    Ok(RawConfig {
        allowed_network,
        share_name,
        path: path_value,
        owner,
        exclude,
    })
}

/// Fill in `owner` and `path` defaults and check them against the system.
pub fn resolve(path: &Path, raw: RawConfig, sys: &dyn System) -> Result<ExportConfig, ConfigError> {
    let key_err = |key: &str, reason: String| ConfigError::InvalidKey {
        path: path.to_path_buf(),
        key: key.to_string(),
        reason,
    };

    let owner = match raw.owner {
        Some(o) => o,
        None => sys.current_user().ok_or_else(|| {
            key_err(
                "owner",
                "not set and the current user is unknown".to_string(),
            )
        })?,
    };
    if owner == "root" {
        return Err(key_err("owner", "must not be root".to_string()));
    }
    let entry = passwd_entry(sys, &owner)
        .ok_or_else(|| key_err("owner", format!("no such user '{owner}'")))?;

    let share_path = raw.path.unwrap_or_else(|| entry.home.clone());
    match sys.dir_owner(&share_path) {
        None => {
            return Err(key_err(
                "path",
                format!("'{}' is not an existing directory", share_path.display()),
            ));
        }
        Some(uid) if uid != entry.uid => {
            return Err(key_err(
                "path",
                format!("'{}' is not owned by {owner}", share_path.display()),
            ));
        }
        Some(_) => {}
    }

    Ok(ExportConfig {
        allowed_network: raw.allowed_network,
        share_name: raw.share_name,
        path: share_path,
        owner,
        exclude: raw.exclude,
    })
}

struct PasswdEntry {
    uid: u32,
    home: PathBuf,
}

fn passwd_entry(sys: &dyn System, user: &str) -> Option<PasswdEntry> {
    let out = sys.run(&Cmd::new("getent").args(["passwd", user])).ok()?;
    if !out.success() {
        return None;
    }
    let fields: Vec<&str> = out.stdout.trim().split(':').collect();
    if fields.len() < 7 {
        return None;
    }
    Some(PasswdEntry {
        uid: fields[2].parse().ok()?,
        home: PathBuf::from(fields[5]),
    })
}

fn display_key(key: &Value) -> String {
    match key {
        Value::String(s) => s.clone(),
        other => serde_yaml_ng::to_string(other)
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::Output;
    use crate::system::fake::FakeSystem;

    fn parse_str(text: &str) -> Result<RawConfig, ConfigError> {
        let path = Path::new("/cfg/export.yaml");
        let mapping = config::parse_mapping(path, text)?;
        parse(path, &mapping)
    }

    #[test]
    fn minimal_file_gets_defaults() {
        let raw = parse_str("allowed_network: 192.168.1.0/24\n").unwrap();
        assert_eq!(raw.allowed_network.to_string(), "192.168.1.0/24");
        assert_eq!(raw.share_name, "home");
        assert_eq!(raw.exclude, vec![".ssh", ".gnupg"]);
        assert_eq!(raw.owner, None);
        assert_eq!(raw.path, None);
    }

    #[test]
    fn full_file_parses() {
        let raw = parse_str(
            "allowed_network: 192.168.1.0/24\nshare_name: home\npath: /home/kostadis\n\
             owner: kostadis\nexclude:\n  - .ssh\n  - .gnupg\n",
        )
        .unwrap();
        assert_eq!(raw.path, Some(PathBuf::from("/home/kostadis")));
        assert_eq!(raw.owner.as_deref(), Some("kostadis"));
    }

    #[test]
    fn host_bits_are_cleared() {
        let raw = parse_str("allowed_network: 192.168.1.85/24\n").unwrap();
        assert_eq!(raw.allowed_network.to_string(), "192.168.1.0/24");
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = parse_str("allowed_network: 10.0.0.0/8\nexlcude: []\n").unwrap_err();
        assert_eq!(err.to_string(), "/cfg/export.yaml: unknown key 'exlcude'");
    }

    #[test]
    fn password_key_is_rejected() {
        let err = parse_str("allowed_network: 10.0.0.0/8\npassword: hunter2\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: passwords are not stored in config; run 'local-machine export setup'"
        );
    }

    #[test]
    fn bad_cidr_is_rejected() {
        let err = parse_str("allowed_network: my-lan\n").unwrap_err();
        assert!(
            err.to_string()
                .starts_with("/cfg/export.yaml: allowed_network:")
        );
    }

    #[test]
    fn missing_allowed_network_is_rejected() {
        let err = parse_str("share_name: home\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: allowed_network: required"
        );
    }

    #[test]
    fn exclude_path_is_rejected() {
        let err =
            parse_str("allowed_network: 10.0.0.0/8\nexclude: [Documents/private]\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: exclude: 'Documents/private' must be a name, not a path"
        );
    }

    #[test]
    fn explicit_empty_exclude_is_allowed() {
        let raw = parse_str("allowed_network: 10.0.0.0/8\nexclude: []\n").unwrap();
        assert!(raw.exclude.is_empty());
    }

    #[test]
    fn resolve_fills_owner_and_home() {
        let sys = FakeSystem::new("kostadis");
        sys.respond(
            "getent passwd kostadis",
            Output::ok("kostadis:x:1000:1000::/home/kostadis:/bin/bash\n"),
        );
        sys.set_dir("/home/kostadis", 1000);
        let raw = parse_str("allowed_network: 10.0.0.0/8\n").unwrap();
        let cfg = resolve(Path::new("/cfg/export.yaml"), raw, &sys).unwrap();
        assert_eq!(cfg.owner, "kostadis");
        assert_eq!(cfg.path, PathBuf::from("/home/kostadis"));
    }

    #[test]
    fn resolve_rejects_root_and_foreign_path() {
        let sys = FakeSystem::new("root");
        let raw = parse_str("allowed_network: 10.0.0.0/8\n").unwrap();
        let err = resolve(Path::new("/cfg/export.yaml"), raw, &sys).unwrap_err();
        assert_eq!(err.to_string(), "/cfg/export.yaml: owner: must not be root");

        let sys = FakeSystem::new("kostadis");
        sys.respond(
            "getent passwd kostadis",
            Output::ok("kostadis:x:1000:1000::/home/kostadis:/bin/bash\n"),
        );
        sys.set_dir("/srv", 0);
        let raw = parse_str("allowed_network: 10.0.0.0/8\npath: /srv\n").unwrap();
        let err = resolve(Path::new("/cfg/export.yaml"), raw, &sys).unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: path: '/srv' is not owned by kostadis"
        );
    }
}
