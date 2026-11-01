//! Parsing `ufw status`, shared by every subcommand that opens the firewall.

use std::path::Path;

use super::System;

/// ufw's own settings file; unlike its rules, it is world-readable.
pub const CONF: &str = "/etc/ufw/ufw.conf";

/// Whether ufw is switched off (`ENABLED=no`), which needs no root to find out. `false`
/// when the file is missing or says anything else: then only `ufw status` can tell.
pub fn off_by_conf(sys: &dyn System) -> bool {
    sys.read_file(Path::new(CONF))
        .is_some_and(|text| text.lines().any(|l| l.trim() == "ENABLED=no"))
}

/// One rule line of `ufw status`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UfwRule {
    /// `Samba`, `8000/tcp`, `OpenSSH (v6)`, ...
    pub to: String,
    /// `ALLOW`, `DENY`, `ALLOW IN`, ...
    pub action: String,
    /// `192.168.1.0/24`, `Anywhere`, `Anywhere (v6)`, ...
    pub from: String,
    /// The text after `#`, if the rule has a comment.
    pub comment: Option<String>,
}

impl UfwRule {
    pub fn allows(&self) -> bool {
        self.action.starts_with("ALLOW")
    }
}

const ACTIONS: &[&str] = &["ALLOW", "DENY", "REJECT", "LIMIT"];

/// `None` when ufw is inactive, else its rules.
pub fn parse_status(stdout: &str) -> Option<Vec<UfwRule>> {
    if !stdout.lines().any(|l| l.trim() == "Status: active") {
        return None;
    }
    let mut rules = Vec::new();
    for line in stdout.lines() {
        let (body, comment) = match line.split_once('#') {
            Some((b, c)) => (b, Some(c.trim().to_string())),
            None => (line, None),
        };
        let tokens: Vec<&str> = body.split_whitespace().collect();
        let Some(pos) = tokens.iter().position(|t| ACTIONS.contains(t)) else {
            continue;
        };
        if pos == 0 {
            continue;
        }
        // `ALLOW IN` / `ALLOW OUT` in verbose output.
        let mut end = pos + 1;
        if matches!(tokens.get(end), Some(&"IN") | Some(&"OUT") | Some(&"FWD")) {
            end += 1;
        }
        if end >= tokens.len() {
            continue;
        }
        rules.push(UfwRule {
            to: tokens[..pos].join(" "),
            action: tokens[pos..end].join(" "),
            from: tokens[end..].join(" "),
            comment,
        });
    }
    Some(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIVE: &str = "Status: active\n\nTo                         Action      From\n\
                          --                         ------      ----\n\
                          OpenSSH                    ALLOW       Anywhere\n\
                          Samba                      ALLOW       192.168.1.0/24\n\
                          8000/tcp                   ALLOW       192.168.1.0/24             # local-machine hypostasis rpg-lib\n\
                          5105/tcp                   DENY        10.0.0.0/8\n\
                          OpenSSH (v6)               ALLOW       Anywhere (v6)\n";

    #[test]
    fn inactive() {
        assert_eq!(parse_status("Status: inactive\n"), None);
    }

    #[test]
    fn app_port_and_comment_rules() {
        let rules = parse_status(ACTIVE).unwrap();
        assert_eq!(rules.len(), 5);
        assert_eq!(rules[1].to, "Samba");
        assert_eq!(rules[1].from, "192.168.1.0/24");
        assert_eq!(rules[1].comment, None);
        assert_eq!(rules[2].to, "8000/tcp");
        assert_eq!(
            rules[2].comment.as_deref(),
            Some("local-machine hypostasis rpg-lib")
        );
        assert!(rules[2].allows());
        assert!(!rules[3].allows());
        assert_eq!(rules[4].to, "OpenSSH (v6)");
        assert_eq!(rules[4].from, "Anywhere (v6)");
    }

    #[test]
    fn verbose_actions() {
        let out = "Status: active\nTo Action From\n8000/tcp ALLOW IN 192.168.1.0/24\n";
        let rules = parse_status(out).unwrap();
        assert_eq!(rules[0].action, "ALLOW IN");
        assert_eq!(rules[0].from, "192.168.1.0/24");
    }
}
