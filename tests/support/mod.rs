//! A simulated Ubuntu machine for `hypostasis` tests, on top of `FakeSystem`.
//!
//! Commands change the fake's state the way the real ones change the machine (a clone makes
//! the checkout, `uv venv` makes the venv, ...), and checks answer from that state, so a
//! scenario can run setup, interrupt it, and run it again.

#![allow(dead_code)]

use local_machine::cli::HypostasisAction;
use local_machine::system::Output;
use local_machine::system::fake::FakeSystem;
use local_machine::{exit_code, hypostasis, output};

pub const HOME: &str = "/home/kostadis";
pub const AUTHORITY: &str = "/home/kostadis/.config/hypostasis/hypostasis.yaml";
pub const AUTHORITY_SOURCE: &str = "/home/kostadis/src/campaigns-private/hypostasis.yaml";
pub const VENV: &str = "/home/kostadis/.venvs/main";
pub const SVENV: &str = "/home/kostadis/.venvs/services";
pub const UV: &str = "/home/kostadis/.local/bin/uv";
pub const MYTOOLS: &str = "/home/kostadis/src/mytools";
pub const DB: &str = "/home/kostadis/src/mytools/rpg-lib/rpg_library.db";
pub const BACKUPS: &str = "/mnt/g/My Drive/hypostasis/backups";
pub const SOCIAL_DATA: &str = "/home/kostadis/data/flexai/social";
pub const COMBAT_DATA: &str = "/home/kostadis/data/flexai/combat";
pub const UNIT_DIR: &str = "/home/kostadis/.config/systemd/user";
pub const LOG_DIR: &str = "/home/kostadis/.local/state/local-machine/hypostasis";
pub const SOCIAL_FILES: &[&str] = &[
    "FlexAI_Social_2021_01_04.xlsx",
    "FlexAI_Social_Choice_DCs_2021_01_04.xlsx",
];
pub const COMBAT_FILES: &[&str] = &["FlexAI_Combat_2021_01_04.xlsx"];
pub const CAMPAIGNS: &[&str] = &["phandalin", "out-of-the-abyss", "obelisk", "toee"];
pub const PACKAGES: &[&str] = &["git", "sqlite3", "nodejs", "npm", "ufw"];

/// (path, repo) of every checkout in tests/fixtures/hypostasis.yaml.
pub const CHECKOUTS: &[(&str, &str)] = &[
    (
        "/home/kostadis/src/mneme",
        "https://github.com/kostadis/mneme.git",
    ),
    (
        "/home/kostadis/src/dgx-fun",
        "https://github.com/kostadis/dgx-fun.git",
    ),
    (
        "/home/kostadis/src/turbovecdb",
        "https://github.com/kostadis/turbovecdb.git",
    ),
    (
        "/home/kostadis/src/Mempalace",
        "https://github.com/kostadis/Mempalace.git",
    ),
    (
        "/home/kostadis/src/CampaignGenerator",
        "https://github.com/kostadis/CampaignGenerator.git",
    ),
    (
        "/home/kostadis/src/mytools",
        "https://github.com/kostadis/mytools.git",
    ),
    (
        "/home/kostadis/src/campaigns-private",
        "https://github.com/kostadis/campaigns-private.git",
    ),
    (
        "/home/kostadis/obelisk",
        "https://github.com/kostadis/campaigns",
    ),
];

pub const SERVICES: &[(&str, u16)] = &[
    ("rpg-lib", 8000),
    ("flexai-social", 5105),
    ("flexai-combat", 5106),
];

/// A marker file standing for state the fake has no other way to hold.
fn mark(sys: &FakeSystem, path: &str) {
    sys.set_file(path, "");
}

fn marked(sys: &FakeSystem, path: &str) -> bool {
    sys.file(path).is_some()
}

fn ok_if(cond: bool, stdout: &str) -> Output {
    if cond {
        Output::ok(stdout)
    } else {
        Output::fail(1, "")
    }
}

fn pkg_marker(pkg: &str) -> String {
    format!("/var/lib/dpkg/{pkg}")
}

pub struct Machine {
    pub config_dir: tempfile::TempDir,
    pub sys: FakeSystem,
}

impl Machine {
    /// A fresh Ubuntu machine: nothing installed, no checkouts; the backups mounted.
    pub fn fresh() -> Self {
        let config_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            config_dir.path().join("machine.yaml"),
            "allowed_network: 192.168.1.0/24\n",
        )
        .unwrap();
        std::fs::write(
            config_dir.path().join("hypostasis.yaml"),
            include_str!("../fixtures/hypostasis.yaml"),
        )
        .unwrap();
        let sys = FakeSystem::new("kostadis");
        sys.respond(
            "getent passwd kostadis",
            Output::ok("kostadis:x:1000:1000::/home/kostadis:/bin/bash\n"),
        );
        sys.respond(
            "ip -o addr show",
            Output::ok("2: enp0s3    inet 192.168.1.85/24 brd 192.168.1.255 scope global enp0s3\n"),
        );
        sys.respond("hostname", Output::ok("linux-alien\n"));
        sys.set_dir(HOME, 1000);
        let m = Machine { config_dir, sys };
        m.simulate();
        m.backups();
        m
    }

    /// A machine on which setup has already run (built by running it).
    pub fn set_up() -> Self {
        let m = Machine::fresh();
        let (code, lines) = m.run(HypostasisAction::Setup);
        assert_eq!(code, Ok(0), "setup on a fresh machine failed: {lines:#?}");
        m.sys.clear_calls();
        m.sys
            .respond("sudo -n -v", Output::fail(1, "a password is required"));
        m
    }

    /// Run an action, capturing its output.
    pub fn run(&self, action: HypostasisAction) -> (Result<u8, u8>, Vec<String>) {
        let (result, mut lines) =
            output::capture(|| hypostasis::run(action, self.config_dir.path(), &self.sys));
        let code = result.map_err(|e| {
            lines.push(format!("error: {e:#}"));
            exit_code(&e)
        });
        (code, lines)
    }

    pub fn backups(&self) {
        let s = &self.sys;
        for dir in [
            "",
            "/rpg-lib",
            "/flexai-social",
            "/flexai-combat",
            "/palaces",
        ] {
            s.set_dir(&format!("{BACKUPS}{dir}"), 1000);
        }
        s.set_file(
            &format!("{BACKUPS}/rpg-lib/rpg_library-2026-10-01.db"),
            "old",
        );
        s.set_file(
            &format!("{BACKUPS}/rpg-lib/rpg_library-2026-11-01.db"),
            "library",
        );
        s.set_file(
            &format!("{BACKUPS}/rpg-lib/user_data-2026-11-01.db"),
            "user",
        );
        for f in SOCIAL_FILES {
            s.set_file(&format!("{BACKUPS}/flexai-social/{f}"), "xlsx");
        }
        for f in COMBAT_FILES {
            s.set_file(&format!("{BACKUPS}/flexai-combat/{f}"), "xlsx");
        }
        for c in CAMPAIGNS {
            let snap = format!("{BACKUPS}/palaces/{c}/20261101-180514");
            s.set_dir(&format!("{BACKUPS}/palaces/{c}"), 1000);
            s.set_dir(&snap, 1000);
            s.set_file(&format!("{snap}/.mneme-backup"), "");
        }
    }

    /// Unmount the backup folder.
    pub fn unmount_backups(&self) {
        self.sys
            .dirs
            .borrow_mut()
            .retain(|p, _| !p.starts_with(BACKUPS));
        self.sys
            .files
            .borrow_mut()
            .retain(|p, _| !p.starts_with(BACKUPS));
    }

    fn simulate(&self) {
        let s = &self.sys;
        s.set_file(AUTHORITY_SOURCE, include_str!("../fixtures/authority.yaml"));

        // Packages: dpkg-query answers from markers; apt-get sets them.
        s.respond_fn("dpkg-query -W -f=${Status} ", |s, line| {
            let pkg = line.rsplit(' ').next().unwrap();
            ok_if(marked(s, &pkg_marker(pkg)), "install ok installed")
        });
        s.on_prefix(
            "sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y ",
            |s, line| {
                let list = line.split(" -y ").nth(1).unwrap();
                for pkg in list.split(' ') {
                    mark(s, &pkg_marker(pkg));
                }
            },
        );
        s.respond_fn("which ", |s, line| {
            let cmd = line.rsplit(' ').next().unwrap();
            ok_if(marked(s, &format!("/usr/bin/{cmd}")), "")
        });
        s.on_prefix("pipx install uv", |s, _| mark(s, UV));

        // Linger.
        s.respond_fn("loginctl show-user kostadis -p Linger", |s, _| {
            Output::ok(if marked(s, "/var/lib/systemd/linger/kostadis") {
                "Linger=yes\n"
            } else {
                "Linger=no\n"
            })
        });
        s.on_prefix("sudo loginctl enable-linger kostadis", |s, _| {
            mark(s, "/var/lib/systemd/linger/kostadis")
        });

        // Checkouts.
        for (path, repo) in CHECKOUTS {
            let (path, repo) = (path.to_string(), repo.to_string());
            let p1 = path.clone();
            s.respond_fn(
                &format!("git -C {path} remote get-url origin"),
                move |s, _| match s.file(&format!("{p1}/.git/origin")) {
                    Some(origin) => Output::ok(&origin),
                    None => Output::fail(2, "error: No such remote 'origin'"),
                },
            );
            let p2 = path.clone();
            let r2 = repo.clone();
            s.on_prefix("git clone ", move |s, line| {
                if line.ends_with(&format!(" {r2} {p2}")) {
                    s.set_dir(&p2, 1000);
                    s.set_dir(&format!("{p2}/.git"), 1000);
                    s.set_file(&format!("{p2}/.git/origin"), &format!("{r2}\n"));
                }
            });
        }

        // ufw: rules held in a marker file.
        let ufw = |s: &FakeSystem, _: &str| {
            let rules = s.file("/etc/ufw/rules").unwrap_or_default();
            Output::ok(&format!(
                "Status: active\n\nTo                         Action      From\n\
                 --                         ------      ----\n\
                 OpenSSH                    ALLOW       Anywhere\n{rules}"
            ))
        };
        s.respond_fn("sudo -n ufw status", ufw);
        s.respond_fn("sudo ufw status", ufw);
        s.on_prefix("sudo ufw allow from ", |s, line| {
            let t: Vec<&str> = line.split(' ').collect();
            // sudo ufw allow from <net> to any port <p> proto tcp comment <words...>
            let (net, port) = (t[4], t[8]);
            let comment = t[12..].join(" ");
            let mut rules = s.file("/etc/ufw/rules").unwrap_or_default();
            rules.push_str(&format!("{port}/tcp ALLOW {net} # {comment}\n"));
            s.set_file("/etc/ufw/rules", &rules);
        });

        // The authority's venv, hypostasis, components.
        s.respond_fn(&format!("{VENV}/bin/python --version"), |s, _| {
            ok_if(s.has_dir(VENV), "Python 3.14.0")
        });
        s.on_prefix(&format!("{UV} venv {VENV}"), |s, _| s.set_dir(VENV, 1000));
        s.respond_fn(&format!("{VENV}/bin/hypostasis --help"), |s, _| {
            ok_if(marked(s, &format!("{VENV}/bin/hypostasis")), "usage")
        });
        s.on_prefix(
            &format!("{UV} pip install --python {VENV}/bin/python -e "),
            |s, _| mark(s, &format!("{VENV}/bin/hypostasis")),
        );
        s.respond_fn(
            &format!("{UV} pip show --python {VENV}/bin/python "),
            |s, _| ok_if(marked(s, &format!("{VENV}/.components")), "Name: x"),
        );
        s.on_prefix(
            &format!("{VENV}/bin/hypostasis install --config "),
            |s, _| mark(s, &format!("{VENV}/.components")),
        );

        // The services venv and installs.
        s.respond_fn(&format!("{SVENV}/bin/python --version"), |s, _| {
            ok_if(s.has_dir(SVENV), "Python 3.14.0")
        });
        s.on_prefix(&format!("{UV} venv {SVENV}"), |s, _| s.set_dir(SVENV, 1000));
        s.respond_fn(
            &format!("{UV} pip show --python {SVENV}/bin/python rpg-lib claudelib"),
            |s, _| ok_if(marked(s, &format!("{SVENV}/.rpg-lib")), "Name: rpg-lib"),
        );
        s.on_prefix(
            &format!("{UV} pip install --python {SVENV}/bin/python -e {MYTOOLS}/claudelib"),
            |s, _| mark(s, &format!("{SVENV}/.rpg-lib")),
        );
        s.on_prefix(
            &format!("npm --prefix {MYTOOLS}/rpg-lib/frontend run build"),
            |s, _| mark(s, &format!("{MYTOOLS}/rpg-lib/frontend/dist/index.html")),
        );
        s.respond_fn(
            &format!("{SVENV}/bin/python -c import flask, openpyxl"),
            |s, _| ok_if(marked(s, &format!("{SVENV}/.flask")), ""),
        );
        s.on_prefix(
            &format!("{UV} pip install --python {SVENV}/bin/python -r "),
            |s, _| mark(s, &format!("{SVENV}/.flask")),
        );

        // Restores.
        s.respond_prefix("sqlite3 ", Output::ok("ok\n"));
        for (app, dir, files) in [
            ("flexai-social", SOCIAL_DATA, SOCIAL_FILES),
            ("flexai-combat", COMBAT_DATA, COMBAT_FILES),
        ] {
            s.respond_fn(
                &format!("{SVENV}/bin/python {MYTOOLS}/{app}/app.py --data-dir {dir} --check"),
                move |s, _| {
                    let missing: Vec<String> = files
                        .iter()
                        .filter(|f| s.file(&format!("{dir}/{f}")).is_none())
                        .map(|f| format!("missing: {f}\n"))
                        .collect();
                    if missing.is_empty() {
                        Output::ok("")
                    } else {
                        Output {
                            code: 1,
                            stdout: missing.concat(),
                            stderr: String::new(),
                        }
                    }
                },
            );
        }
        for c in CAMPAIGNS {
            let c = c.to_string();
            let c2 = c.clone();
            s.respond_fn(&format!("{VENV}/bin/mneme mp status {c} "), move |s, _| {
                let state = if marked(s, &format!("/palaces/{c}")) {
                    "built                      store healthy"
                } else {
                    "missing_config             not provisioned"
                };
                Output::ok(&format!("ok   {c}              store   {state}\n"))
            });
            s.on_prefix(
                &format!("{VENV}/bin/mneme mp restore {c2} "),
                move |s, _| mark(s, &format!("/palaces/{c2}")),
            );
        }
    }

    // ---- service runtime ----

    /// Make `systemctl --user enable --now` start a service that then answers.
    pub fn services_start_cleanly(&self) {
        for (name, port) in SERVICES {
            let unit = format!("local-machine-{name}.service");
            let port = *port;
            let u2 = unit.clone();
            self.sys.respond_fn(
                &format!("systemctl --user is-active {unit}"),
                move |s, _| {
                    Output::ok(if marked(s, &format!("/run/units/{u2}")) {
                        "active\n"
                    } else {
                        "inactive\n"
                    })
                },
            );
            let u3 = unit.clone();
            self.sys.respond_fn(
                &format!("systemctl --user is-enabled {unit}"),
                move |s, _| {
                    Output::ok(if marked(s, &format!("/run/enabled/{u3}")) {
                        "enabled\n"
                    } else {
                        "disabled\n"
                    })
                },
            );
            let u4 = unit.clone();
            self.sys.on_prefix(
                &format!("systemctl --user enable --now {unit}"),
                move |s, _| {
                    mark(s, &format!("/run/units/{u4}"));
                    mark(s, &format!("/run/enabled/{u4}"));
                    s.set_http(port, &[None, None, Some(200)]);
                },
            );
            let u5 = unit.clone();
            self.sys.on_prefix(
                &format!("systemctl --user disable --now {unit}"),
                move |s, _| {
                    s.files
                        .borrow_mut()
                        .remove(std::path::Path::new(&format!("/run/units/{u5}")));
                    s.files
                        .borrow_mut()
                        .remove(std::path::Path::new(&format!("/run/enabled/{u5}")));
                    s.set_http(port, &[Some(200), None]);
                },
            );
        }
    }

    pub fn calls_matching(&self, needle: &str) -> Vec<String> {
        self.sys
            .calls()
            .into_iter()
            .filter(|c| c.contains(needle))
            .collect()
    }

    /// Calls that change something (not reads or checks).
    pub fn mutating_calls(&self) -> Vec<String> {
        const READS: &[&str] = &[
            "getent ",
            "ip -o addr",
            "hostname",
            "dpkg-query ",
            "which ",
            "loginctl show-user",
            "git -C ",
            "sudo -n ",
            "systemctl --user is-",
            "ss -ltnpH",
            "sqlite3 ",
        ];
        self.sys
            .calls()
            .into_iter()
            .filter(|c| !READS.iter().any(|r| c.starts_with(r)))
            .filter(|c| !c.ends_with(" --version") && !c.ends_with(" --help"))
            .filter(|c| !c.contains(" pip show ") && !c.contains(" -c import "))
            .filter(|c| !c.ends_with(" --check") && !c.contains(" mp status "))
            .collect()
    }
}

pub fn has_line(lines: &[String], line: &str) -> bool {
    lines.iter().any(|l| l == line)
}

pub fn position(lines: &[String], pred: impl Fn(&str) -> bool) -> usize {
    lines
        .iter()
        .position(|l| pred(l))
        .unwrap_or_else(|| panic!("no matching line in {lines:#?}"))
}
