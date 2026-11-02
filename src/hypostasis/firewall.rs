//! `firewall <service>`: open each service's port to the home network only (FR-030,
//! research R8). The only setup steps besides packages and linger that need root.

use std::cell::Cell;

use anyhow::Result;
use ipnet::IpNet;

use crate::system::action::{Check, Step};
use crate::system::ufw::{self, UfwRule};
use crate::system::{Cmd, System, run_ok};

use super::Context;
use super::services::{self, Kind};

const INACTIVE: &str = "ufw inactive; services reachable from any network";

pub fn comment(kind: Kind) -> String {
    format!("local-machine hypostasis {}", kind.name())
}

/// What the firewall needs for one service.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Done,
    Inactive,
    /// Our rules to delete (stale port or network), and whether the wanted rule is missing.
    Change {
        stale: Vec<UfwRule>,
        add: bool,
    },
}

pub fn judge(rules: Option<Vec<UfwRule>>, kind: Kind, port: u16, net: &IpNet) -> Verdict {
    let Some(rules) = rules else {
        return Verdict::Inactive;
    };
    let wanted_to = format!("{port}/tcp");
    let wanted_from = net.to_string();
    let ours = comment(kind);
    let present = rules
        .iter()
        .any(|r| r.allows() && r.to == wanted_to && r.from == wanted_from);
    let stale: Vec<UfwRule> = rules
        .into_iter()
        .filter(|r| r.comment.as_deref() == Some(ours.as_str()))
        .filter(|r| !(r.to == wanted_to && r.from == wanted_from))
        .filter(|r| !r.to.contains("(v6)"))
        .collect();
    if present && stale.is_empty() {
        Verdict::Done
    } else {
        Verdict::Change {
            stale,
            add: !present,
        }
    }
}

pub struct FirewallStep<'a> {
    ctx: &'a Context,
    kind: Kind,
    name: String,
    /// The port the last check saw, for the privilege notice.
    seen_port: Cell<Option<u16>>,
}

impl<'a> FirewallStep<'a> {
    pub fn new(ctx: &'a Context, kind: Kind) -> Self {
        FirewallStep {
            ctx,
            kind,
            name: format!("firewall {}", kind.name()),
            seen_port: Cell::new(None),
        }
    }

    fn port(&self, sys: &dyn System) -> Result<Option<u16>> {
        let authority = match self.kind {
            Kind::RpgLib => self.ctx.authority(sys)?,
            _ => None,
        };
        Ok(services::port(self.ctx, self.kind, authority.as_ref()))
    }
}

impl Step for FirewallStep<'_> {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self, sys: &dyn System) -> Result<Check> {
        let port = self.port(sys)?;
        self.seen_port.set(port);
        let Some(port) = port else {
            return Ok(Check::NeedsChange(format!(
                "needs the authority file {}",
                self.ctx.cfg.authority.display()
            )));
        };
        if ufw::off_by_conf(sys) {
            return Ok(Check::DoneWith(INACTIVE.into()));
        }
        let out = match sys.run(&Cmd::new("ufw").args(["status"]).sudo_n()) {
            Ok(o) if o.success() => o,
            _ => return Ok(Check::Unknown),
        };
        Ok(
            match judge(
                ufw::parse_status(&out.stdout),
                self.kind,
                port,
                &self.ctx.machine.allowed_network,
            ) {
                Verdict::Done => Check::Done,
                Verdict::Inactive => Check::DoneWith(INACTIVE.into()),
                Verdict::Change { stale, .. } if !stale.is_empty() => {
                    Check::NeedsChange("config changed".into())
                }
                Verdict::Change { .. } => Check::NeedsChange(format!(
                    "port {port}/tcp not open to {}",
                    self.ctx.machine.allowed_network
                )),
            },
        )
    }

    fn notice(&self, found: &Check) -> Option<String> {
        let net = &self.ctx.machine.allowed_network;
        let port = match self.seen_port.get() {
            Some(p) => p.to_string(),
            None => format!("rpg-lib's port (from {})", self.ctx.cfg.authority.display()),
        };
        Some(match found {
            Check::Unknown => format!(
                "check ufw; allow port {port}/tcp from {net} ({}) if ufw is active",
                self.kind.name()
            ),
            _ => format!("allow port {port}/tcp from {net} ({})", self.kind.name()),
        })
    }

    fn apply(&mut self, sys: &dyn System) -> Result<()> {
        let Some(port) = self.port(sys)? else {
            anyhow::bail!(
                "rpg-lib's port is unknown: {} does not exist",
                self.ctx.cfg.authority.display()
            );
        };
        let out = run_ok(sys, &Cmd::new("ufw").args(["status"]).sudo())?;
        let net = self.ctx.machine.allowed_network;
        let Verdict::Change { stale, add } =
            judge(ufw::parse_status(&out.stdout), self.kind, port, &net)
        else {
            return Ok(());
        };
        for rule in stale {
            let old_port = rule.to.trim_end_matches("/tcp").to_string();
            run_ok(
                sys,
                &Cmd::new("ufw")
                    .args([
                        "delete", "allow", "from", &rule.from, "to", "any", "port", &old_port,
                        "proto", "tcp",
                    ])
                    .sudo(),
            )?;
        }
        if add {
            run_ok(
                sys,
                &Cmd::new("ufw")
                    .args([
                        "allow",
                        "from",
                        &net.to_string(),
                        "to",
                        "any",
                        "port",
                        &port.to_string(),
                        "proto",
                        "tcp",
                        "comment",
                        &comment(self.kind),
                    ])
                    .sudo(),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> Option<Vec<UfwRule>> {
        ufw::parse_status(&format!("Status: active\nTo Action From\n{text}"))
    }

    fn net() -> IpNet {
        "192.168.1.0/24".parse().unwrap()
    }

    #[test]
    fn present_rule_from_the_network_is_done() {
        let r = rules("8000/tcp ALLOW 192.168.1.0/24 # local-machine hypostasis rpg-lib\n");
        assert_eq!(judge(r, Kind::RpgLib, 8000, &net()), Verdict::Done);
    }

    #[test]
    fn inactive_is_a_note() {
        assert_eq!(judge(None, Kind::RpgLib, 8000, &net()), Verdict::Inactive);
    }

    #[test]
    fn our_rule_for_another_network_or_port_is_stale() {
        let r = rules(
            "8000/tcp ALLOW 10.0.0.0/8 # local-machine hypostasis rpg-lib\n\
             8001/tcp ALLOW 192.168.1.0/24 # local-machine hypostasis rpg-lib\n\
             5105/tcp ALLOW 10.0.0.0/8 # local-machine hypostasis flexai-social\n",
        );
        match judge(r, Kind::RpgLib, 8000, &net()) {
            Verdict::Change { stale, add } => {
                assert!(add);
                assert_eq!(stale.len(), 2);
                assert_eq!(stale[1].to, "8001/tcp");
            }
            v => panic!("{v:?}"),
        }
    }

    #[test]
    fn someone_elses_rule_is_left_alone() {
        let r = rules("8000/tcp ALLOW Anywhere\n");
        assert_eq!(
            judge(r, Kind::RpgLib, 8000, &net()),
            Verdict::Change {
                stale: vec![],
                add: true
            }
        );
    }
}
