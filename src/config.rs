//! Locating and reading subcommand configuration (constitution Principle II).
//!
//! Each subcommand reads its own file, `<config-dir>/<subcommand>.yaml`.

use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no config at {} (need at least {need})", path.display())]
    Missing { path: PathBuf, need: String },
    #[error("{}: {reason}", path.display())]
    Invalid { path: PathBuf, reason: String },
    #[error("{}: {key}: {reason}", path.display())]
    InvalidKey {
        path: PathBuf,
        key: String,
        reason: String,
    },
}

/// `--config-dir` if given, else `$HOME/.config/local-machine`.
pub fn resolve_dir(flag: Option<&Path>) -> PathBuf {
    match flag {
        Some(dir) => dir.to_path_buf(),
        None => {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config").join("local-machine")
        }
    }
}

pub fn file_for(dir: &Path, subcommand: &str) -> PathBuf {
    dir.join(format!("{subcommand}.yaml"))
}

/// Read `<dir>/<subcommand>.yaml` as a YAML mapping. `need` names the required keys for
/// the "missing file" message.
pub fn read_mapping(
    dir: &Path,
    subcommand: &str,
    need: &str,
) -> Result<(PathBuf, Mapping), ConfigError> {
    let path = file_for(dir, subcommand);
    let text = std::fs::read_to_string(&path).map_err(|_| ConfigError::Missing {
        path: path.clone(),
        need: need.to_string(),
    })?;
    let mapping = parse_mapping(&path, &text)?;
    Ok((path, mapping))
}

pub fn parse_mapping(path: &Path, text: &str) -> Result<Mapping, ConfigError> {
    let value: Value = serde_yaml_ng::from_str(text).map_err(|e| ConfigError::Invalid {
        path: path.to_path_buf(),
        reason: format!("not valid YAML: {e}"),
    })?;
    match value {
        Value::Mapping(m) => Ok(m),
        Value::Null => Ok(Mapping::new()),
        _ => Err(ConfigError::Invalid {
            path: path.to_path_buf(),
            reason: "expected a mapping of keys to values".to_string(),
        }),
    }
}
