//! `<config-dir>/export.yaml` (contracts/export-config.md, data-model.md → ExportConfig).
//!
//! `allowed_network` and `owner` come from the shared `machine.yaml` (feature 002).

use std::path::{Path, PathBuf};

use ipnet::IpNet;
use serde_yaml_ng::{Mapping, Value};

use crate::config::{self, ConfigError, display_key};
use crate::machine::{self, MachineConfig};
use crate::system::System;

pub const SUBCOMMAND: &str = "export";
const KEYS: &[&str] = &["share_name", "path", "exclude"];
/// Keys whose home is now `machine.yaml`.
const MOVED: &[&str] = &["allowed_network", "owner"];
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

/// The file as written, before defaults that need the system (path).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawConfig {
    pub share_name: String,
    pub path: Option<PathBuf>,
    pub exclude: Vec<String>,
}

pub fn load(config_dir: &Path, sys: &dyn System) -> Result<ExportConfig, ConfigError> {
    let machine = machine::load(config_dir, sys)?;
    let (path, mapping) = config::read_mapping_optional(config_dir, SUBCOMMAND)?;
    let raw = parse(config_dir, &path, &mapping.unwrap_or_default())?;
    resolve(&path, raw, &machine, sys)
}

pub fn parse(config_dir: &Path, path: &Path, mapping: &Mapping) -> Result<RawConfig, ConfigError> {
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
        if MOVED.contains(&name) {
            return Err(key_err(
                name,
                format!("moved to {}", machine::path(config_dir).display()),
            ));
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
        share_name,
        path: path_value,
        exclude,
    })
}

/// Fill in the `path` default (the owner's home) and check it against the system.
pub fn resolve(
    path: &Path,
    raw: RawConfig,
    machine: &MachineConfig,
    sys: &dyn System,
) -> Result<ExportConfig, ConfigError> {
    let key_err = |key: &str, reason: String| ConfigError::InvalidKey {
        path: path.to_path_buf(),
        key: key.to_string(),
        reason,
    };
    let owner = machine.owner.clone();
    let share_path = raw.path.unwrap_or_else(|| machine.owner_home.clone());
    match sys.dir_owner(&share_path) {
        None => {
            return Err(key_err(
                "path",
                format!("'{}' is not an existing directory", share_path.display()),
            ));
        }
        Some(uid) if uid != machine.owner_uid => {
            return Err(key_err(
                "path",
                format!("'{}' is not owned by {owner}", share_path.display()),
            ));
        }
        Some(_) => {}
    }

    Ok(ExportConfig {
        allowed_network: machine.allowed_network,
        share_name: raw.share_name,
        path: share_path,
        owner,
        exclude: raw.exclude,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "/cfg";

    fn parse_str(text: &str) -> Result<RawConfig, ConfigError> {
        let path = Path::new("/cfg/export.yaml");
        let mapping = config::parse_mapping(path, text)?;
        parse(Path::new(DIR), path, &mapping)
    }

    fn machine() -> MachineConfig {
        MachineConfig {
            allowed_network: "10.0.0.0/8".parse().unwrap(),
            owner: "kostadis".to_string(),
            owner_uid: 1000,
            owner_home: PathBuf::from("/home/kostadis"),
        }
    }

    #[test]
    fn empty_file_gets_defaults() {
        let raw = parse_str("").unwrap();
        assert_eq!(raw.share_name, "home");
        assert_eq!(raw.exclude, vec![".ssh", ".gnupg"]);
        assert_eq!(raw.path, None);
    }

    #[test]
    fn full_file_parses() {
        let raw =
            parse_str("share_name: home\npath: /home/kostadis\nexclude:\n  - .ssh\n  - .gnupg\n")
                .unwrap();
        assert_eq!(raw.path, Some(PathBuf::from("/home/kostadis")));
    }

    #[test]
    fn moved_keys_name_machine_yaml() {
        let err = parse_str("allowed_network: 192.168.1.0/24\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: allowed_network: moved to /cfg/machine.yaml"
        );
        let err = parse_str("owner: kostadis\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: owner: moved to /cfg/machine.yaml"
        );
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = parse_str("exlcude: []\n").unwrap_err();
        assert_eq!(err.to_string(), "/cfg/export.yaml: unknown key 'exlcude'");
    }

    #[test]
    fn password_key_is_rejected() {
        let err = parse_str("password: hunter2\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: passwords are not stored in config; run 'local-machine export setup'"
        );
    }

    #[test]
    fn exclude_path_is_rejected() {
        let err = parse_str("exclude: [Documents/private]\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: exclude: 'Documents/private' must be a name, not a path"
        );
    }

    #[test]
    fn explicit_empty_exclude_is_allowed() {
        let raw = parse_str("exclude: []\n").unwrap();
        assert!(raw.exclude.is_empty());
    }

    #[test]
    fn resolve_takes_network_and_owner_from_machine() {
        let sys = crate::system::fake::FakeSystem::new("kostadis");
        sys.set_dir("/home/kostadis", 1000);
        let raw = parse_str("").unwrap();
        let cfg = resolve(Path::new("/cfg/export.yaml"), raw, &machine(), &sys).unwrap();
        assert_eq!(cfg.owner, "kostadis");
        assert_eq!(cfg.allowed_network.to_string(), "10.0.0.0/8");
        assert_eq!(cfg.path, PathBuf::from("/home/kostadis"));
    }

    #[test]
    fn resolve_rejects_a_foreign_path() {
        let sys = crate::system::fake::FakeSystem::new("kostadis");
        sys.set_dir("/srv", 0);
        let raw = parse_str("path: /srv\n").unwrap();
        let err = resolve(Path::new("/cfg/export.yaml"), raw, &machine(), &sys).unwrap_err();
        assert_eq!(
            err.to_string(),
            "/cfg/export.yaml: path: '/srv' is not owned by kostadis"
        );
    }

    #[test]
    fn a_missing_export_yaml_means_all_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("machine.yaml"),
            "allowed_network: 10.0.0.0/8\n",
        )
        .unwrap();
        let sys = crate::system::fake::FakeSystem::new("kostadis");
        sys.respond(
            "getent passwd kostadis",
            crate::system::Output::ok("kostadis:x:1000:1000::/home/kostadis:/bin/bash\n"),
        );
        sys.set_dir("/home/kostadis", 1000);
        let cfg = load(dir.path(), &sys).unwrap();
        assert_eq!(cfg.share_name, "home");
        assert_eq!(cfg.path, PathBuf::from("/home/kostadis"));
    }
}
