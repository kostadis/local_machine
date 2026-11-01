//! A read-only view of hypostasis's authority file (data-model.md → Authority).
//!
//! Only the keys below are read; everything else is ignored. This module never writes.
//! Constitution Principle II allows reading another tool's own config at a path named in our
//! config, for values that tool owns.

use std::path::{Path, PathBuf};

use serde_yaml_ng::{Mapping, Value};

use crate::config::{ConfigError, expand_tilde};
use crate::system::{Cmd, PathKind, System, net};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authority {
    pub path: PathBuf,
    pub venv: PathBuf,
    pub installer: String,
    pub rpg_lib_url: String,
    pub rpg_lib_port: u16,
    pub rpg_lib_health_path: String,
    pub rpg_library_db: PathBuf,
    pub campaigns: Vec<PathBuf>,
    pub component_paths: Vec<PathBuf>,
    pub component_names: Vec<String>,
}

impl Authority {
    /// Campaign names, as `mneme` knows them. Each `data_roots.campaigns` entry is a search
    /// root; a campaign is a folder directly inside it that holds `.mneme`
    /// (`~/phandalin/Phandalin` → `Phandalin`). A root with none falls back to its own last
    /// part, so a missing campaign tree still shows up as a failing piece.
    pub fn campaign_names(&self, sys: &dyn System) -> Vec<String> {
        let mut names = Vec::new();
        for root in &self.campaigns {
            let found: Vec<String> = sys
                .list_dir(root)
                .into_iter()
                .filter(|n| sys.path_kind(&root.join(n).join(".mneme")) == PathKind::Dir)
                .collect();
            if found.is_empty() {
                if let Some(n) = root.file_name() {
                    names.push(n.to_string_lossy().into_owned());
                }
            } else {
                names.extend(found);
            }
        }
        names.dedup();
        names
    }

    pub fn python(&self) -> PathBuf {
        self.venv.join("bin").join("python")
    }

    pub fn bin(&self, program: &str) -> PathBuf {
        self.venv.join("bin").join(program)
    }
}

/// Names this machine answers to: `localhost`, loopback, its hostname, its addresses.
pub fn local_hosts(sys: &dyn System) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Ok(out) = sys.run(&Cmd::new("hostname"))
        && out.success()
        && !out.stdout.trim().is_empty()
    {
        hosts.push(out.stdout.trim().to_string());
    }
    hosts.extend(net::local_addresses(sys).iter().map(|a| a.to_string()));
    hosts
}

/// Read the authority at `path`. `Ok(None)` when the file does not exist yet.
pub fn load_if_present(
    sys: &dyn System,
    path: &Path,
    home: &Path,
) -> Result<Option<Authority>, ConfigError> {
    if sys.path_kind(path) == PathKind::Missing {
        return Ok(None);
    }
    load(sys, path, home, &local_hosts(sys)).map(Some)
}

pub fn load(
    sys: &dyn System,
    path: &Path,
    home: &Path,
    local_hosts: &[String],
) -> Result<Authority, ConfigError> {
    let invalid = |reason: String| ConfigError::Invalid {
        path: path.to_path_buf(),
        reason: format!("hypostasis's authority file: {reason}"),
    };
    let text = sys
        .read_file(path)
        .ok_or_else(|| invalid("missing or unreadable".to_string()))?;
    parse(path, &text, home, local_hosts)
}

fn get<'a>(m: &'a Mapping, dotted: &str) -> Option<&'a Value> {
    let mut parts = dotted.split('.');
    let mut v = m.get(parts.next()?)?;
    for p in parts {
        v = v.as_mapping()?.get(p)?;
    }
    Some(v)
}

pub fn parse(
    path: &Path,
    text: &str,
    home: &Path,
    local_hosts: &[String],
) -> Result<Authority, ConfigError> {
    let invalid = |reason: String| ConfigError::Invalid {
        path: path.to_path_buf(),
        reason: format!("hypostasis's authority file: {reason}"),
    };
    let value: Value =
        serde_yaml_ng::from_str(text).map_err(|e| invalid(format!("not valid YAML: {e}")))?;
    let Value::Mapping(m) = value else {
        return Err(invalid("expected a mapping".to_string()));
    };
    let string = |key: &str| -> Result<String, ConfigError> {
        match get(&m, key) {
            Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().to_string()),
            None | Some(Value::Null) => Err(invalid(format!("{key} is missing"))),
            Some(_) => Err(invalid(format!("{key} must be a string"))),
        }
    };
    let path_of = |key: &str| -> Result<PathBuf, ConfigError> {
        let p = expand_tilde(&string(key)?, home);
        if !p.is_absolute() {
            return Err(invalid(format!("{key} must be an absolute path")));
        }
        Ok(p)
    };

    let venv = path_of("venv")?;
    let installer = string("installer")?;
    if installer != "uv" {
        return Err(invalid(format!(
            "installer '{installer}' is not supported (only uv)"
        )));
    }

    let rpg_lib_port = match get(&m, "services.rpg_lib.port") {
        Some(Value::Number(n)) => match n.as_u64() {
            Some(p) if (1..=65535).contains(&p) => p as u16,
            _ => return Err(invalid(format!("services.rpg_lib.port {n} is not a port"))),
        },
        _ => return Err(invalid("services.rpg_lib.port is missing".to_string())),
    };
    let rpg_lib_url = string("services.rpg_lib.url")?;
    let rpg_lib_health_path = match get(&m, "services.rpg_lib.health.path") {
        Some(Value::String(s)) if s.starts_with('/') => s.clone(),
        None | Some(Value::Null) => "/".to_string(),
        Some(_) => {
            return Err(invalid(
                "services.rpg_lib.health.path must start with '/'".to_string(),
            ));
        }
    };
    check_rpg_lib_url(&rpg_lib_url, rpg_lib_port, local_hosts).map_err(invalid)?;

    let rpg_library_db = path_of("data_roots.rpg_library_db")?;

    let campaigns = match get(&m, "data_roots.campaigns") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Sequence(items)) => items
            .iter()
            .map(|i| {
                i.as_str()
                    .map(|s| expand_tilde(s, home))
                    .ok_or_else(|| invalid("data_roots.campaigns must be paths".to_string()))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(invalid("data_roots.campaigns must be a list".to_string())),
    };

    let mut component_paths = Vec::new();
    if let Some(Value::Mapping(components)) = m.get("components") {
        for (_, comp) in components {
            if let Some(Value::String(p)) = comp
                .as_mapping()
                .and_then(|c| c.get("source"))
                .and_then(|s| s.as_mapping())
                .and_then(|s| s.get("path"))
            {
                component_paths.push(expand_tilde(p, home));
            }
        }
    }

    let component_names = match get(&m, "order.install") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        Some(_) => return Err(invalid("order.install must be a list".to_string())),
    };

    Ok(Authority {
        path: path.to_path_buf(),
        venv,
        installer,
        rpg_lib_url,
        rpg_lib_port,
        rpg_lib_health_path,
        rpg_library_db,
        campaigns,
        component_paths,
        component_names,
    })
}

/// rpg-lib must run on this machine, on the port the authority names (analysis I3).
fn check_rpg_lib_url(url: &str, port: u16, local_hosts: &[String]) -> Result<(), String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("services.rpg_lib.url '{url}' is not a URL"))?;
    let authority = rest.split('/').next().unwrap_or("");
    let (host, url_port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6
            .split_once(']')
            .ok_or_else(|| format!("services.rpg_lib.url '{url}' is not a URL"))?;
        (h.to_string(), after.strip_prefix(':').map(str::to_string))
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p.to_string())),
            None => (authority.to_string(), None),
        }
    };
    let url_port: u16 = match url_port {
        Some(p) => p
            .parse()
            .map_err(|_| format!("services.rpg_lib.url '{url}' has a bad port"))?,
        None if scheme == "https" => 443,
        None => 80,
    };
    if !local_hosts.iter().any(|h| h.eq_ignore_ascii_case(&host)) {
        return Err(format!(
            "services.rpg_lib.url '{url}' is not this machine; rpg-lib must run on this machine"
        ));
    }
    if url_port != port {
        return Err(format!(
            "services.rpg_lib.url '{url}' has port {url_port} but services.rpg_lib.port is {port}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/authority.yaml");
    const HOME: &str = "/home/kostadis";

    fn hosts() -> Vec<String> {
        [
            "localhost",
            "127.0.0.1",
            "::1",
            "Linux-Alien",
            "192.168.1.85",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn parse_str(text: &str) -> Result<Authority, ConfigError> {
        parse(
            Path::new("/a/hypostasis.yaml"),
            text,
            Path::new(HOME),
            &hosts(),
        )
    }

    #[test]
    fn this_machines_file_parses() {
        let a = parse_str(FIXTURE).unwrap();
        assert_eq!(a.venv, PathBuf::from("/home/kostadis/.venvs/main"));
        assert_eq!(a.installer, "uv");
        assert_eq!(a.rpg_lib_port, 8000);
        assert_eq!(a.rpg_lib_health_path, "/");
        assert_eq!(
            a.rpg_library_db,
            PathBuf::from("/home/kostadis/src/mytools/rpg-lib/rpg_library.db")
        );
        let sys = crate::system::fake::FakeSystem::new("kostadis");
        assert_eq!(
            a.campaign_names(&sys),
            ["phandalin", "out-of-the-abyss", "obelisk", "toee"]
        );
        sys.set_dir("/home/kostadis/phandalin/Phandalin", 1000);
        sys.set_dir("/home/kostadis/phandalin/Phandalin/.mneme", 1000);
        sys.set_dir("/home/kostadis/phandalin/scripts", 1000);
        assert_eq!(
            a.campaign_names(&sys),
            ["Phandalin", "out-of-the-abyss", "obelisk", "toee"]
        );
        assert_eq!(a.component_paths.len(), 4);
        assert!(
            a.component_paths
                .contains(&PathBuf::from("/home/kostadis/src/Mempalace"))
        );
        assert_eq!(
            a.component_names,
            ["dgxlib", "turbovecdb", "mempalace", "CampaignGenerator"]
        );
    }

    fn with_url(url: &str) -> Result<Authority, ConfigError> {
        parse_str(&FIXTURE.replace("url: http://localhost:8000", &format!("url: {url}")))
    }

    #[test]
    fn rpg_lib_url_on_this_machine() {
        assert!(with_url("http://localhost:8000").is_ok());
        assert!(with_url("http://192.168.1.85:8000").is_ok());
        assert!(with_url("http://linux-alien:8000/").is_ok());
        assert!(with_url("http://[::1]:8000").is_ok());
    }

    #[test]
    fn rpg_lib_url_on_another_host_is_refused() {
        let e = with_url("http://192.168.1.121:8000")
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("is not this machine; rpg-lib must run on this machine"),
            "{e}"
        );
        assert!(e.contains("hypostasis's authority file"), "{e}");
    }

    #[test]
    fn rpg_lib_url_port_mismatch_is_refused() {
        let e = with_url("http://localhost:9000").unwrap_err().to_string();
        assert!(
            e.contains("has port 9000 but services.rpg_lib.port is 8000"),
            "{e}"
        );
    }

    #[test]
    fn other_installer_is_refused() {
        let e = parse_str(&FIXTURE.replace("installer: uv", "installer: pip"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("installer 'pip' is not supported"), "{e}");
    }

    #[test]
    fn missing_file_names_the_path() {
        let sys = crate::system::fake::FakeSystem::new("kostadis");
        let e = load(
            &sys,
            Path::new("/a/hypostasis.yaml"),
            Path::new(HOME),
            &hosts(),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            e,
            "/a/hypostasis.yaml: hypostasis's authority file: missing or unreadable"
        );
    }
}
