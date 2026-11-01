//! `<config-dir>/machine.yaml`: facts about this machine that more than one subcommand
//! uses (constitution Principle II; specs/002-hypostasis-setup/contracts/machine-config.md).

use std::path::{Path, PathBuf};
use std::str::FromStr;

use ipnet::IpNet;
use serde_yaml_ng::{Mapping, Value};

use crate::config::{self, ConfigError, display_key};
use crate::system::{Cmd, System};

pub const FILE: &str = "machine";
const KEYS: &[&str] = &["allowed_network", "owner"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineConfig {
    /// The trusted home network.
    pub allowed_network: IpNet,
    /// The one human account; never root.
    pub owner: String,
    pub owner_uid: u32,
    pub owner_home: PathBuf,
}

/// The file as written, before defaults that need the system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawMachine {
    pub allowed_network: IpNet,
    pub owner: Option<String>,
}

/// The path of the shared file in `config_dir`.
pub fn path(config_dir: &Path) -> PathBuf {
    config::file_for(config_dir, FILE)
}

pub fn load(config_dir: &Path, sys: &dyn System) -> Result<MachineConfig, ConfigError> {
    let (path, mapping) = config::read_mapping(config_dir, FILE, "allowed_network")?;
    let raw = parse(&path, &mapping)?;
    resolve(&path, raw, sys)
}

pub fn parse(path: &Path, mapping: &Mapping) -> Result<RawMachine, ConfigError> {
    let key_err = |key: &str, reason: String| ConfigError::InvalidKey {
        path: path.to_path_buf(),
        key: key.to_string(),
        reason,
    };
    for key in mapping.keys() {
        if !KEYS.contains(&key.as_str().unwrap_or("")) {
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
    Ok(RawMachine {
        allowed_network,
        owner: string("owner")?,
    })
}

/// Fill in the `owner` default and check it against the system.
pub fn resolve(
    path: &Path,
    raw: RawMachine,
    sys: &dyn System,
) -> Result<MachineConfig, ConfigError> {
    let key_err = |reason: String| ConfigError::InvalidKey {
        path: path.to_path_buf(),
        key: "owner".to_string(),
        reason,
    };
    let owner = match raw.owner {
        Some(o) => o,
        None => sys
            .current_user()
            .ok_or_else(|| key_err("not set and the current user is unknown".to_string()))?,
    };
    if owner == "root" {
        return Err(key_err("must not be root".to_string()));
    }
    let entry =
        passwd_entry(sys, &owner).ok_or_else(|| key_err(format!("no such user '{owner}'")))?;
    Ok(MachineConfig {
        allowed_network: raw.allowed_network,
        owner,
        owner_uid: entry.uid,
        owner_home: entry.home,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswdEntry {
    pub uid: u32,
    pub home: PathBuf,
}

/// `getent passwd <user>`.
pub fn passwd_entry(sys: &dyn System, user: &str) -> Option<PasswdEntry> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::Output;
    use crate::system::fake::FakeSystem;

    const P: &str = "/cfg/machine.yaml";

    fn load_str(text: &str, sys: &FakeSystem) -> Result<MachineConfig, ConfigError> {
        let path = Path::new(P);
        let mapping = config::parse_mapping(path, text)?;
        resolve(path, parse(path, &mapping)?, sys)
    }

    fn sys() -> FakeSystem {
        let sys = FakeSystem::new("kostadis");
        sys.respond(
            "getent passwd kostadis",
            Output::ok("kostadis:x:1000:1000::/home/kostadis:/bin/bash\n"),
        );
        sys.respond_prefix("getent passwd", Output::fail(2, ""));
        sys
    }

    #[test]
    fn minimal_file_defaults_owner_to_the_current_user() {
        let cfg = load_str("allowed_network: 192.168.1.85/24\n", &sys()).unwrap();
        assert_eq!(cfg.allowed_network.to_string(), "192.168.1.0/24");
        assert_eq!(cfg.owner, "kostadis");
        assert_eq!(cfg.owner_uid, 1000);
        assert_eq!(cfg.owner_home, PathBuf::from("/home/kostadis"));
    }

    #[test]
    fn owner_set() {
        let cfg = load_str("allowed_network: 10.0.0.0/8\nowner: kostadis\n", &sys()).unwrap();
        assert_eq!(cfg.owner, "kostadis");
    }

    #[test]
    fn owner_root_is_rejected() {
        let err = load_str("allowed_network: 10.0.0.0/8\nowner: root\n", &sys()).unwrap_err();
        assert_eq!(err.to_string(), format!("{P}: owner: must not be root"));
    }

    #[test]
    fn unknown_owner_is_rejected() {
        let err = load_str("allowed_network: 10.0.0.0/8\nowner: nobody2\n", &sys()).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("{P}: owner: no such user 'nobody2'")
        );
    }

    #[test]
    fn bad_cidr_is_rejected() {
        let err = load_str("allowed_network: my-lan\n", &sys()).unwrap_err();
        assert!(
            err.to_string()
                .starts_with(&format!("{P}: allowed_network:"))
        );
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = load_str("allowed_network: 10.0.0.0/8\nshare_name: x\n", &sys()).unwrap_err();
        assert_eq!(err.to_string(), format!("{P}: unknown key 'share_name'"));
    }

    #[test]
    fn missing_file_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(dir.path(), &sys()).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "no config at {}/machine.yaml (need at least allowed_network)",
                dir.path().display()
            )
        );
    }
}
