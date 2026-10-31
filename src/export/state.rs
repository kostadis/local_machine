//! Setup components and share state, derived from the system (data-model.md).
//!
//! Root-only checks run with `sudo -n` and report `Unknown` instead of prompting.

use std::path::Path;

use crate::ExitError;
use crate::system::action::{Gate, GateResult};
use crate::system::{Cmd, System};

use super::config::ExportConfig;
use super::samba;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Presence {
    Present,
    /// Present, with a note (e.g. firewall inactive).
    PresentWith(String),
    Missing {
        reason: String,
        /// The component exists but no longer matches the configuration.
        config_changed: bool,
    },
    Unknown,
}

impl Presence {
    fn missing(reason: impl Into<String>) -> Self {
        Presence::Missing {
            reason: reason.into(),
            config_changed: false,
        }
    }

    fn changed(reason: impl Into<String>) -> Self {
        Presence::Missing {
            reason: reason.into(),
            config_changed: true,
        }
    }
}

pub const SAMBA_PACKAGE: &str = "samba-package";
pub const GLOBAL_CONFIG: &str = "global-config";
pub const INCLUDE_LINE: &str = "include-line";
pub const SAMBA_USER: &str = "samba-user";
pub const FIREWALL: &str = "firewall";
pub const NMBD_DISABLED: &str = "nmbd-disabled";

pub fn check(name: &str, cfg: &ExportConfig, sys: &dyn System) -> Presence {
    match name {
        SAMBA_PACKAGE => samba_package(sys),
        GLOBAL_CONFIG => global_config(cfg, sys),
        INCLUDE_LINE => include_line(sys),
        SAMBA_USER => samba_user(cfg, sys),
        FIREWALL => firewall(cfg, sys),
        NMBD_DISABLED => nmbd_disabled(sys),
        _ => unreachable!("unknown component {name}"),
    }
}

pub const COMPONENTS: &[&str] = &[
    SAMBA_PACKAGE,
    GLOBAL_CONFIG,
    INCLUDE_LINE,
    SAMBA_USER,
    FIREWALL,
    NMBD_DISABLED,
];

pub fn samba_package(sys: &dyn System) -> Presence {
    let out = sys.run(&Cmd::new("dpkg-query").args(["-W", "-f=${Status}", "samba"]));
    match out {
        Ok(o) if o.success() && o.stdout.trim() == "install ok installed" => Presence::Present,
        _ => Presence::missing("samba is not installed"),
    }
}

fn global_config(cfg: &ExportConfig, sys: &dyn System) -> Presence {
    match sys.read_file(Path::new(samba::GLOBALS_PATH)) {
        None => Presence::missing(format!("{} missing", samba::GLOBALS_PATH)),
        Some(text) if text == samba::render_globals(cfg) => Presence::Present,
        Some(_) => Presence::changed("configuration changed"),
    }
}

pub fn include_count(smb_conf: &str) -> usize {
    let wanted = samba::include_line();
    smb_conf.lines().filter(|l| l.trim() == wanted).count()
}

pub fn include_line(sys: &dyn System) -> Presence {
    match sys.read_file(Path::new(samba::SMB_CONF)) {
        None => Presence::missing(format!("{} missing", samba::SMB_CONF)),
        Some(text) => match include_count(&text) {
            1 => Presence::Present,
            0 => Presence::missing(format!("{} does not include our settings", samba::SMB_CONF)),
            n => Presence::missing(format!(
                "{} includes our settings {n} times",
                samba::SMB_CONF
            )),
        },
    }
}

fn samba_user(cfg: &ExportConfig, sys: &dyn System) -> Presence {
    match sys.run(&Cmd::new("pdbedit").args(["-L"]).sudo_n()) {
        Ok(o) if o.success() => {
            let prefix = format!("{}:", cfg.owner);
            if o.stdout.lines().any(|l| l.starts_with(&prefix)) {
                Presence::Present
            } else {
                Presence::missing(format!("no Samba password for {}", cfg.owner))
            }
        }
        _ => Presence::Unknown,
    }
}

/// Parsed `ufw status`: `None` when inactive, else the sources of `Samba` rules.
pub fn parse_ufw(stdout: &str) -> Option<Vec<String>> {
    if !stdout.lines().any(|l| l.trim() == "Status: active") {
        return None;
    }
    let mut sources = Vec::new();
    for line in stdout.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.first() == Some(&"Samba")
            && tokens.contains(&"ALLOW")
            && let Some(from) = tokens.last()
        {
            sources.push(from.to_string());
        }
    }
    Some(sources)
}

fn firewall(cfg: &ExportConfig, sys: &dyn System) -> Presence {
    let out = match sys.run(&Cmd::new("ufw").args(["status"]).sudo_n()) {
        Ok(o) if o.success() => o,
        _ => return Presence::Unknown,
    };
    let wanted = cfg.allowed_network.to_string();
    match parse_ufw(&out.stdout) {
        None => Presence::PresentWith("ufw inactive; Samba hosts allow still applies".to_string()),
        Some(sources) if sources == [wanted.clone()] => Presence::Present,
        Some(sources) if sources.iter().any(|s| *s != wanted) => {
            Presence::changed(format!("firewall allows Samba from {}", sources.join(", ")))
        }
        Some(_) => Presence::missing(format!("firewall does not allow Samba from {wanted}")),
    }
}

pub fn nmbd_disabled(sys: &dyn System) -> Presence {
    match unit_enabled(sys, "nmbd") {
        true => Presence::missing("nmbd is enabled"),
        false => Presence::Present,
    }
}

pub fn unit_enabled(sys: &dyn System, unit: &str) -> bool {
    sys.run(&Cmd::new("systemctl").args(["is-enabled", unit]))
        .map(|o| o.stdout.trim() == "enabled")
        .unwrap_or(false)
}

pub fn unit_active(sys: &dyn System, unit: &str) -> bool {
    sys.run(&Cmd::new("systemctl").args(["is-active", unit]))
        .map(|o| o.stdout.trim() == "active")
        .unwrap_or(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareState {
    NotSetUp,
    SetUp,
    Available,
}

#[derive(Debug)]
pub struct Report {
    pub state: ShareState,
    pub missing: Vec<(&'static str, String)>,
    pub unknown: Vec<&'static str>,
    pub notes: Vec<(&'static str, String)>,
    /// A setup-level config change (allowed_network, owner): run `export setup`.
    pub config_changed: bool,
    /// The share file holds a stanza that differs from the current config: run `export start`.
    pub share_drift: bool,
    /// The share file holds a stanza (anything beyond the header).
    pub share_defined: bool,
    pub smbd_enabled: bool,
    pub smbd_active: bool,
}

pub fn detect(cfg: &ExportConfig, sys: &dyn System) -> Report {
    let mut missing = Vec::new();
    let mut unknown = Vec::new();
    let mut notes = Vec::new();
    let mut config_changed = false;
    for &name in COMPONENTS {
        match check(name, cfg, sys) {
            Presence::Present => {}
            Presence::PresentWith(note) => notes.push((name, note)),
            Presence::Missing {
                reason,
                config_changed: changed,
            } => {
                config_changed |= changed;
                missing.push((name, reason));
            }
            Presence::Unknown => unknown.push(name),
        }
    }

    let share_text = sys.read_file(Path::new(samba::SHARE_PATH));
    let share_defined = share_text
        .as_deref()
        .is_some_and(|t| t != samba::render_empty_share());
    let share_matches = share_text.as_deref() == Some(samba::render_share(cfg).as_str());
    let smbd_enabled = unit_enabled(sys, "smbd");
    let smbd_active = unit_active(sys, "smbd");

    let state = if !missing.is_empty() {
        ShareState::NotSetUp
    } else if share_matches && smbd_enabled && smbd_active {
        ShareState::Available
    } else {
        ShareState::SetUp
    };

    Report {
        state,
        missing,
        unknown,
        notes,
        config_changed,
        share_drift: share_defined && !share_matches,
        share_defined,
        smbd_enabled,
        smbd_active,
    }
}

/// `start` and `stop` run only once `export setup` has completed.
pub struct SetUp<'a>(pub &'a ExportConfig);

impl Gate for SetUp<'_> {
    fn check(&self, sys: &dyn System) -> GateResult {
        let report = detect(self.0, sys);
        if report.state == ShareState::NotSetUp {
            GateResult::Refuse(ExitError::new(
                ExitError::NOT_SET_UP,
                report.not_set_up_message(),
            ))
        } else if !report.unknown.is_empty() {
            GateResult::Unknown(vec![format!(
                "check that setup is complete ({})",
                report.unknown.join(", ")
            )])
        } else {
            GateResult::Pass
        }
    }
}

impl Report {
    /// The "not set up" message, naming `export setup` as the fix (FR-005).
    pub fn not_set_up_message(&self) -> String {
        let headline = if self.config_changed {
            "export: configuration changed — run 'local-machine export setup'"
        } else {
            "export: not set up — run 'local-machine export setup'"
        };
        let mut msg = headline.to_string();
        for (name, reason) in &self.missing {
            msg.push_str(&format!("\n  missing: {name} ({reason})"));
        }
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ufw_inactive() {
        assert_eq!(parse_ufw("Status: inactive\n"), None);
    }

    #[test]
    fn ufw_samba_rules() {
        let out = "Status: active\n\nTo                         Action      From\n\
                   --                         ------      ----\n\
                   OpenSSH                    ALLOW       Anywhere\n\
                   Samba                      ALLOW       192.168.1.0/24\n";
        assert_eq!(parse_ufw(out), Some(vec!["192.168.1.0/24".to_string()]));
    }

    #[test]
    fn include_counted_exactly() {
        let line = samba::include_line();
        assert_eq!(include_count(&format!("[global]\n{line}\n")), 1);
        assert_eq!(include_count(&format!("{line}\n  {line}\n")), 2);
        assert_eq!(include_count("[global]\n"), 0);
    }
}
