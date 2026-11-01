//! The three local services: rpg-lib, flexai-social, flexai-combat (data-model.md →
//! LocalService, research R6). Each runs as a systemd user unit owned by the owner.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::system::{Cmd, System};

use super::Context;
use super::authority::Authority;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    RpgLib,
    FlexSocial,
    FlexCombat,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::RpgLib, Kind::FlexSocial, Kind::FlexCombat];

    pub fn name(self) -> &'static str {
        match self {
            Kind::RpgLib => "rpg-lib",
            Kind::FlexSocial => "flexai-social",
            Kind::FlexCombat => "flexai-combat",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalService {
    pub kind: Kind,
    pub name: &'static str,
    pub port: u16,
    pub health_path: String,
    pub unit_name: String,
    pub unit_path: PathBuf,
    pub log_path: PathBuf,
}

pub fn unit_dir(home: &Path) -> PathBuf {
    home.join(".config/systemd/user")
}

pub fn log_dir(home: &Path) -> PathBuf {
    home.join(".local/state/local-machine/hypostasis")
}

/// The port of a service: rpg-lib's comes from the authority (None before it exists).
pub fn port(ctx: &Context, kind: Kind, authority: Option<&Authority>) -> Option<u16> {
    match kind {
        Kind::RpgLib => authority.map(|a| a.rpg_lib_port),
        Kind::FlexSocial => Some(ctx.cfg.flexai_social.port),
        Kind::FlexCombat => Some(ctx.cfg.flexai_combat.port),
    }
}

pub fn service(ctx: &Context, kind: Kind, authority: &Authority) -> LocalService {
    let home = ctx.home();
    let unit_name = format!("local-machine-{}.service", kind.name());
    LocalService {
        kind,
        name: kind.name(),
        port: port(ctx, kind, Some(authority)).expect("authority given"),
        health_path: match kind {
            Kind::RpgLib => authority.rpg_lib_health_path.clone(),
            _ => "/".to_string(),
        },
        unit_path: unit_dir(home).join(&unit_name),
        unit_name,
        log_path: log_dir(home).join(format!("{}.log", kind.name())),
    }
}

pub fn table(ctx: &Context, authority: &Authority) -> Vec<LocalService> {
    Kind::ALL
        .iter()
        .map(|&k| service(ctx, k, authority))
        .collect()
}

/// The service's app directory inside mytools.
pub fn app_dir(ctx: &Context, kind: Kind) -> PathBuf {
    ctx.cfg.mytools.join(kind.name())
}

pub fn services_python(ctx: &Context) -> PathBuf {
    ctx.cfg.services_venv.join("bin").join("python")
}

/// One `ExecStart` word, quoted when it holds whitespace or quotes.
fn word(s: &str) -> String {
    if s.is_empty()
        || s.chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// The user unit for `svc`, exactly as setup writes it (research R6).
pub fn render_unit(ctx: &Context, svc: &LocalService, authority: &Authority) -> String {
    let py = services_python(ctx);
    let app = app_dir(ctx, svc.kind);
    let port = svc.port.to_string();
    let (args, env): (Vec<String>, Vec<(String, String)>) = match svc.kind {
        Kind::RpgLib => (
            vec![
                py.display().to_string(),
                app.join("library_server.py").display().to_string(),
                "--db".to_string(),
                authority.rpg_library_db.display().to_string(),
                "--host".to_string(),
                "0.0.0.0".to_string(),
                "--port".to_string(),
                port,
            ],
            Vec::new(),
        ),
        Kind::FlexSocial | Kind::FlexCombat => {
            let flex = match svc.kind {
                Kind::FlexSocial => &ctx.cfg.flexai_social,
                _ => &ctx.cfg.flexai_combat,
            };
            (
                vec![
                    py.display().to_string(),
                    app.join("app.py").display().to_string(),
                    "--data-dir".to_string(),
                    flex.data_dir.display().to_string(),
                    "--host".to_string(),
                    "0.0.0.0".to_string(),
                    "--port".to_string(),
                    port,
                ],
                flex.env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )
        }
    };
    let mut text = String::new();
    text.push_str("# Written by 'local-machine hypostasis setup'; changes are replaced.\n");
    text.push_str("[Unit]\n");
    text.push_str(&format!(
        "Description=local-machine hypostasis: {}\n",
        svc.name
    ));
    text.push_str("\n[Service]\n");
    text.push_str(&format!("WorkingDirectory={}\n", app.display()));
    for (k, v) in env {
        text.push_str(&format!("Environment={}\n", word(&format!("{k}={v}"))));
    }
    let exec: Vec<String> = args.iter().map(|a| word(a)).collect();
    text.push_str(&format!("ExecStart={}\n", exec.join(" ")));
    text.push_str("Restart=on-failure\n");
    text.push_str(&format!(
        "StandardOutput=append:{}\n",
        svc.log_path.display()
    ));
    text.push_str(&format!(
        "StandardError=append:{}\n",
        svc.log_path.display()
    ));
    text.push_str("\n[Install]\nWantedBy=default.target\n");
    text
}

/// Answering: an HTTP status below 500 within 2 s.
pub fn probe(sys: &dyn System, svc: &LocalService) -> bool {
    sys.http_status(svc.port, &svc.health_path, Duration::from_secs(2))
        .is_some_and(|s| s < 500)
}

fn systemctl_user(sys: &dyn System, verb: &str, unit: &str) -> String {
    sys.run(&Cmd::new("systemctl").args(["--user", verb, unit]))
        .map(|o| o.stdout.trim().to_string())
        .unwrap_or_default()
}

pub fn unit_active(sys: &dyn System, svc: &LocalService) -> bool {
    systemctl_user(sys, "is-active", &svc.unit_name) == "active"
}

pub fn unit_enabled(sys: &dyn System, svc: &LocalService) -> bool {
    systemctl_user(sys, "is-enabled", &svc.unit_name) == "enabled"
}

pub fn unit_failed(sys: &dyn System, svc: &LocalService) -> bool {
    systemctl_user(sys, "is-failed", &svc.unit_name) == "failed"
}

/// Who is listening on `port`, from `ss -ltnpH sport = :<port>`.
pub fn port_holder(sys: &dyn System, port: u16) -> Option<String> {
    let out = sys
        .run(&Cmd::new("ss").args(["-ltnpH", "sport", "=", &format!(":{port}")]))
        .ok()?;
    parse_ss(&out.stdout)
}

/// `None` when nothing listens; the process name when visible, else "another process".
pub fn parse_ss(stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|l| !l.trim().is_empty())?;
    if let Some(start) = line.find("users:((\"") {
        let rest = &line[start + "users:((\"".len()..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    Some("another process".to_string())
}

/// The last `n` lines of a log file (fewer when it is shorter; none when it is missing).
pub fn log_tail(sys: &dyn System, path: &Path, n: usize) -> Vec<String> {
    let Some(text) = sys.read_file(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| l.to_string())
        .collect()
}

/// The address clients use: this machine's address on the home network, else loopback.
pub fn url(ctx: &Context, sys: &dyn System, port: u16) -> String {
    let host = crate::system::net::local_address(&ctx.machine.allowed_network, sys)
        .map(|a| a.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    format!("http://{host}:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hypostasis::test_support::{authority, context};

    #[test]
    fn rpg_lib_unit() {
        let ctx = context();
        let a = authority();
        let svc = service(&ctx, Kind::RpgLib, &a);
        assert_eq!(svc.unit_name, "local-machine-rpg-lib.service");
        assert_eq!(
            render_unit(&ctx, &svc, &a),
            "# Written by 'local-machine hypostasis setup'; changes are replaced.\n\
             [Unit]\n\
             Description=local-machine hypostasis: rpg-lib\n\
             \n\
             [Service]\n\
             WorkingDirectory=/home/kostadis/src/mytools/rpg-lib\n\
             ExecStart=/home/kostadis/.venvs/services/bin/python /home/kostadis/src/mytools/rpg-lib/library_server.py --db /home/kostadis/src/mytools/rpg-lib/rpg_library.db --host 0.0.0.0 --port 8000\n\
             Restart=on-failure\n\
             StandardOutput=append:/home/kostadis/.local/state/local-machine/hypostasis/rpg-lib.log\n\
             StandardError=append:/home/kostadis/.local/state/local-machine/hypostasis/rpg-lib.log\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n"
        );
    }

    #[test]
    fn flexai_social_unit_has_sorted_environment() {
        let ctx = context();
        let a = authority();
        let svc = service(&ctx, Kind::FlexSocial, &a);
        let unit = render_unit(&ctx, &svc, &a);
        assert!(unit.contains(
            "WorkingDirectory=/home/kostadis/src/mytools/flexai-social\n\
             Environment=CHAT_URL=http://192.168.1.121:8003/v1\n\
             Environment=CLEF_URL=http://192.168.1.121:8002/v1\n\
             ExecStart=/home/kostadis/.venvs/services/bin/python /home/kostadis/src/mytools/flexai-social/app.py --data-dir /home/kostadis/data/flexai/social --host 0.0.0.0 --port 5105\n"
        ), "{unit}");
    }

    #[test]
    fn flexai_combat_unit_quotes_spaces() {
        let mut ctx = context();
        ctx.cfg.flexai_combat.data_dir = PathBuf::from("/mnt/g/My Drive/combat");
        let a = authority();
        let svc = service(&ctx, Kind::FlexCombat, &a);
        let unit = render_unit(&ctx, &svc, &a);
        assert!(
            unit.contains("--data-dir \"/mnt/g/My Drive/combat\" --host 0.0.0.0 --port 5106\n"),
            "{unit}"
        );
        assert!(!unit.contains("Environment="));
    }

    #[test]
    fn ss_output() {
        assert_eq!(parse_ss(""), None);
        assert_eq!(
            parse_ss(
                "LISTEN 0 2048 127.0.0.1:8000 0.0.0.0:* users:((\"python\",pid=1098703,fd=6))\n"
            ),
            Some("python".to_string())
        );
        assert_eq!(
            parse_ss("LISTEN 0 5 0.0.0.0:5106 0.0.0.0:*\n"),
            Some("another process".to_string())
        );
    }

    #[test]
    fn short_log_tail() {
        let sys = crate::system::fake::FakeSystem::new("kostadis");
        sys.set_file("/log", "a\nb\nc\n");
        assert_eq!(log_tail(&sys, Path::new("/log"), 20), ["a", "b", "c"]);
        assert_eq!(log_tail(&sys, Path::new("/log"), 2), ["b", "c"]);
        assert!(log_tail(&sys, Path::new("/nope"), 20).is_empty());
    }
}
