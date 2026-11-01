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

/// Like [`read_mapping`], but a missing file is `None` (all defaults).
pub fn read_mapping_optional(
    dir: &Path,
    subcommand: &str,
) -> Result<(PathBuf, Option<Mapping>), ConfigError> {
    let path = file_for(dir, subcommand);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let mapping = parse_mapping(&path, &text)?;
            Ok((path, Some(mapping)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((path, None)),
        Err(e) => Err(ConfigError::Invalid {
            path: path.clone(),
            reason: format!("cannot read: {e}"),
        }),
    }
}

/// `~` or `~/x` → under `home`; anything else unchanged.
pub fn expand_tilde(value: &str, home: &Path) -> PathBuf {
    if value == "~" {
        home.to_path_buf()
    } else if let Some(rest) = value.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(value)
    }
}

/// A YAML key as written, for messages.
pub fn display_key(key: &Value) -> String {
    match key {
        Value::String(s) => s.clone(),
        other => serde_yaml_ng::to_string(other)
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion() {
        let home = Path::new("/home/me");
        assert_eq!(expand_tilde("~", home), PathBuf::from("/home/me"));
        assert_eq!(
            expand_tilde("~/src/x", home),
            PathBuf::from("/home/me/src/x")
        );
        assert_eq!(expand_tilde("/abs", home), PathBuf::from("/abs"));
        assert_eq!(expand_tilde("~other/x", home), PathBuf::from("~other/x"));
    }
}
