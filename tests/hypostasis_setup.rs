//! `hypostasis setup` on a simulated machine: fresh (US1, T025) and re-runs (US2, T027).

mod support;

use local_machine::cli::HypostasisAction;
use local_machine::system::Output;
use support::*;

fn setup(m: &Machine) -> (Result<u8, u8>, Vec<String>) {
    m.run(HypostasisAction::Setup)
}

const ORDER: &[&str] = &[
    "packages",
    "uv",
    "linger",
    "checkout /home/kostadis/src/mneme",
    "checkout /home/kostadis/obelisk",
    "authority",
    "firewall rpg-lib",
    "firewall flexai-social",
    "firewall flexai-combat",
    "component-paths",
    "venv",
    "hypostasis",
    "components",
    "services-venv",
    "install rpg-lib",
    "install flexai-social",
    "install flexai-combat",
    "unit rpg-lib",
    "unit flexai-social",
    "unit flexai-combat",
    "restore rpg-library",
    "restore flexai-social data",
    "restore flexai-combat data",
    "restore palace phandalin",
    "restore palace toee",
];

// ---------------------------------------------------------------- US1: fresh machine

#[test]
fn fresh_machine_end_to_end() {
    let m = Machine::fresh();
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");

    // Pieces in data-model order.
    let mut last = 0;
    for name in ORDER {
        let at = position(&lines, |l| {
            l == format!("changed: {name}") || l == format!("ok: {name}")
        });
        assert!(at > last || last == 0, "{name} out of order: {lines:#?}");
        last = at;
    }
    // `adding:` before each `changed:`.
    let adding = position(&lines, |l| l == "adding: venv");
    assert_eq!(lines[adding + 1], "changed: venv");

    // One notice listing only root work, before any root command.
    assert!(m.sys.prompted());
    let notice = position(&lines, |l| l == "needs administrator privileges for:");
    let mut root_lines = Vec::new();
    for l in &lines[notice + 1..] {
        match l.strip_prefix("  ") {
            Some(r) => root_lines.push(r.to_string()),
            None => break,
        }
    }
    assert_eq!(
        root_lines,
        [
            "install packages: git sqlite3 nodejs npm ufw",
            "enable linger for kostadis",
            "allow port rpg-lib's port (from /home/kostadis/.config/hypostasis/hypostasis.yaml)/tcp from 192.168.1.0/24 (rpg-lib)",
            "allow port 5105/tcp from 192.168.1.0/24 (flexai-social)",
            "allow port 5106/tcp from 192.168.1.0/24 (flexai-combat)",
        ]
    );

    // Every sudo call is apt, linger or ufw, and all come before the first `uv venv`.
    let calls = m.sys.calls();
    let first_venv = calls
        .iter()
        .position(|c| c.contains(" venv "))
        .expect("a uv venv call");
    for (i, c) in calls.iter().enumerate() {
        if c.starts_with("sudo ") && !c.starts_with("sudo -n ") && !c.starts_with("sudo -v") {
            assert!(i < first_venv, "{c} after uv venv");
            assert!(
                c.contains("apt-get install")
                    || c.starts_with("sudo loginctl enable-linger")
                    || c.starts_with("sudo ufw "),
                "unexpected root call {c}"
            );
        }
    }
    assert!(
        m.sys
            .calls()
            .iter()
            .all(|c| !c.starts_with("sudo -n ") || c == "sudo -n -v" || c == "sudo -n ufw status"),
        "unexpected sudo -n call"
    );

    // Starts nothing.
    for verb in ["start", "enable", "restart"] {
        assert!(
            m.calls_matching(&format!("systemctl --user {verb}"))
                .is_empty(),
            "setup ran systemctl --user {verb}"
        );
    }

    // Ends with the next steps.
    let n = lines.len();
    assert_eq!(lines[n - 2], "next: local-machine hypostasis start");
    assert_eq!(lines[n - 1], "then: mneme up <campaign>");

    // The firewall rules carry our comment.
    let rules = m.sys.file("/etc/ufw/rules").unwrap();
    assert!(rules.contains("8000/tcp ALLOW 192.168.1.0/24 # local-machine hypostasis rpg-lib"));

    // Restores landed, newest backup first, no partial left behind.
    assert_eq!(m.sys.file(DB).as_deref(), Some("library"));
    assert_eq!(
        m.sys
            .file("/home/kostadis/src/mytools/rpg-lib/user_data.db")
            .as_deref(),
        Some("user")
    );
    assert!(m.sys.file(&format!("{DB}.partial")).is_none());
    for f in SOCIAL_FILES {
        assert!(m.sys.file(&format!("{SOCIAL_DATA}/{f}")).is_some());
    }
    assert_eq!(m.calls_matching("mp restore").len(), CAMPAIGNS.len());
}

#[test]
fn authority_is_seeded_byte_for_byte() {
    let m = Machine::fresh();
    assert_eq!(setup(&m).0, Ok(0));
    assert_eq!(m.sys.file(AUTHORITY), m.sys.file(AUTHORITY_SOURCE));
}

#[test]
fn unreadable_authority_source_fails_before_the_venv() {
    let m = Machine::fresh();
    m.sys
        .files
        .borrow_mut()
        .remove(std::path::Path::new(AUTHORITY_SOURCE));
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains(AUTHORITY_SOURCE) && l.contains("not readable")),
        "{lines:#?}"
    );
    assert!(m.calls_matching(" venv ").is_empty());
    assert!(has_line(&lines, "done: 11 pieces"), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("remaining: authority, firewall rpg-lib")),
        "{lines:#?}"
    );
}

#[test]
fn uncovered_component_path_fails_before_the_venv() {
    let m = Machine::fresh();
    let text =
        include_str!("fixtures/authority.yaml").replace("~/src/turbovecdb", "~/src/elsewhere");
    m.sys.set_file(AUTHORITY_SOURCE, &text);
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("component-paths") && l.contains("/home/kostadis/src/elsewhere")),
        "{lines:#?}"
    );
    assert!(m.calls_matching(" venv ").is_empty());
}

#[test]
fn hypostasis_install_failure_shows_its_stderr_and_restores_nothing() {
    let m = Machine::fresh();
    m.sys
        .respond_fn(&format!("{VENV}/bin/hypostasis install"), |_, _| {
            Output::fail(2, "component mempalace: pin mismatch")
        });
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("component mempalace: pin mismatch")),
        "{lines:#?}"
    );
    assert!(m.sys.file(DB).is_none());
    assert!(m.calls_matching("--check").is_empty() || m.calls_matching("cp ").is_empty());
    assert!(m.calls_matching("mp restore").is_empty());
    assert!(
        !lines.iter().any(|l| l.starts_with("adding: restore")),
        "{lines:#?}"
    );
}

#[test]
fn unmounted_backups_fail_every_restore_after_the_software() {
    let m = Machine::fresh();
    m.unmount_backups();
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        has_line(&lines, "changed: unit flexai-combat"),
        "{lines:#?}"
    );
    let restores = [
        "restore rpg-library",
        "restore flexai-social data",
        "restore flexai-combat data",
    ];
    for r in restores {
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with(&format!("error: {r}: backup folder"))
                    && l.contains("mount ")),
            "{r}: {lines:#?}"
        );
    }
    for c in CAMPAIGNS {
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with(&format!("error: restore palace {c}:"))),
            "{lines:#?}"
        );
    }
    assert!(
        lines.last().unwrap().contains("7 steps failed"),
        "{lines:#?}"
    );

    // Mount and re-run: only the restores run.
    m.backups();
    m.sys.clear_calls();
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(!m.sys.prompted());
    let changed: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("changed: "))
        .collect();
    assert!(
        changed.iter().all(|l| l.starts_with("changed: restore ")),
        "{changed:#?}"
    );
}

#[test]
fn corrupt_db_backup_leaves_no_partial_and_no_target() {
    let m = Machine::fresh();
    m.sys.respond_fn("sqlite3 ", |_, _| {
        Output::ok("*** in database main ***\nPage 3: btreeInitPage() returns error code 11\n")
    });
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(lines.iter().any(|l| l.starts_with("error: restore rpg-library:") && l.contains("failed its check")), "{lines:#?}");
    assert!(m.sys.file(DB).is_none());
    assert!(m.sys.file(&format!("{DB}.partial")).is_none());
    // The other restores still ran.
    assert!(
        has_line(&lines, "changed: restore flexai-social data"),
        "{lines:#?}"
    );
}

#[test]
fn palace_without_snapshot_is_named_and_others_still_run() {
    let m = Machine::fresh();
    m.sys
        .dirs
        .borrow_mut()
        .retain(|p, _| !p.starts_with(format!("{BACKUPS}/palaces/toee/")));
    m.sys
        .files
        .borrow_mut()
        .retain(|p, _| !p.starts_with(format!("{BACKUPS}/palaces/toee/")));
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        lines.iter().any(|l| l
            == &format!("error: restore palace toee: no mneme snapshot in {BACKUPS}/palaces/toee")),
        "{lines:#?}"
    );
    assert!(
        has_line(&lines, "changed: restore palace obelisk"),
        "{lines:#?}"
    );
}

// ---------------------------------------------------------------- US2: re-runs

#[test]
fn fully_set_up_changes_nothing_and_never_prompts() {
    let m = Machine::set_up();
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(!m.sys.prompted());
    assert!(m.mutating_calls().is_empty(), "{:#?}", m.mutating_calls());
    assert!(
        lines
            .iter()
            .all(|l| l.starts_with("ok: ") || l.starts_with("next: ") || l.starts_with("then: ")),
        "{lines:#?}"
    );
}

#[test]
fn existing_checkout_gets_only_read_only_git_commands() {
    let m = Machine::set_up();
    // A different branch or a dirty tree changes nothing setup looks at.
    let (code, _) = setup(&m);
    assert_eq!(code, Ok(0));
    for c in m.calls_matching("git ") {
        assert!(c.ends_with("remote get-url origin"), "{c}");
    }
}

#[test]
fn occupied_paths_are_never_touched() {
    for (setup_path, found) in [
        ("file", "a file"),
        ("dir", "not a git repository"),
        ("origin", "origin is https://github.com/someone/else"),
    ] {
        let m = Machine::fresh();
        let p = "/home/kostadis/src/turbovecdb";
        match setup_path {
            "file" => m.sys.set_file(p, "x"),
            "dir" => m.sys.set_dir(p, 1000),
            _ => {
                m.sys.set_dir(p, 1000);
                m.sys.set_dir(&format!("{p}/.git"), 1000);
                m.sys.set_file(
                    &format!("{p}/.git/origin"),
                    "https://github.com/someone/else\n",
                );
            }
        }
        let (code, lines) = setup(&m);
        assert_eq!(code, Err(1), "{lines:#?}");
        assert!(
            lines
                .iter()
                .any(|l| l == &format!("error: checkout {p}: occupied ({found}); not touched")),
            "{found}: {lines:#?}"
        );
        for c in m.sys.calls() {
            assert!(
                !(c.contains(p) && !c.ends_with("remote get-url origin")),
                "touched {p}: {c}"
            );
        }
        // A fresh machine has root work pending, but the refusal comes first (FR-014).
        assert!(!m.sys.prompted(), "{found}: {lines:#?}");
        assert!(
            m.mutating_calls().is_empty(),
            "{found}: {:#?}",
            m.mutating_calls()
        );
        assert!(
            !lines.iter().any(|l| l.starts_with("needs administrator")),
            "{lines:#?}"
        );
    }
}

#[test]
fn every_occupied_path_is_named_in_one_refusal() {
    let m = Machine::fresh();
    for p in [
        "/home/kostadis/src/turbovecdb",
        "/home/kostadis/src/mytools",
    ] {
        m.sys.set_file(p, "x");
    }
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        lines.iter().any(|l| l
            == "error: checkout /home/kostadis/src/turbovecdb: occupied (a file); not touched\n\
                error: checkout /home/kostadis/src/mytools: occupied (a file); not touched"),
        "{lines:#?}"
    );
    assert!(!m.sys.prompted());
}

#[test]
fn ufw_switched_off_is_known_without_root() {
    let m = Machine::set_up();
    m.sys
        .set_file("/etc/ufw/ufw.conf", "# comment\nENABLED=no\nLOGLEVEL=low\n");
    m.sys.respond_fn("sudo -n ufw status", |_, _| {
        Output::fail(1, "a password is required")
    });
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(!m.sys.prompted(), "{lines:#?}");
    assert!(has_line(
        &lines,
        "ok: firewall rpg-lib (ufw inactive; services reachable from any network)"
    ));
}

#[test]
fn active_ufw_without_cached_sudo_asks_to_verify() {
    // Its rules are root-only, so finding out is root work (constitution VI, SC-002).
    let m = Machine::set_up();
    m.sys.set_file("/etc/ufw/ufw.conf", "ENABLED=yes\n");
    m.sys.respond_fn("sudo -n ufw status", |_, _| {
        Output::fail(1, "a password is required")
    });
    let (_, lines) = setup(&m);
    assert!(m.sys.prompted(), "{lines:#?}");
    let notice = position(&lines, |l| l == "needs administrator privileges for:");
    assert_eq!(
        lines[notice + 1],
        "  check ufw; allow port 8000/tcp from 192.168.1.0/24 (rpg-lib) if ufw is active"
    );
}

#[test]
fn an_interrupted_run_resumes_with_only_the_remaining_steps() {
    let m = Machine::fresh();
    // The 5th piece (the second checkout) fails once.
    m.sys.respond(
        "git clone --branch main https://github.com/kostadis/dgx-fun.git /home/kostadis/src/dgx-fun",
        Output::fail(128, "fatal: unable to access"),
    );
    let (code, lines) = setup(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(has_line(&lines, "done: 4 pieces"), "{lines:#?}");

    m.sys.responses.borrow_mut().remove(
        "git clone --branch main https://github.com/kostadis/dgx-fun.git /home/kostadis/src/dgx-fun",
    );
    m.sys.clear_calls();
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    for done in [
        "packages",
        "uv",
        "linger",
        "checkout /home/kostadis/src/mneme",
    ] {
        assert!(
            has_line(&lines, &format!("ok: {done}")),
            "{done}: {lines:#?}"
        );
    }
    assert!(has_line(
        &lines,
        "changed: checkout /home/kostadis/src/dgx-fun"
    ));
    assert!(m.calls_matching("apt-get").is_empty());
}

#[test]
fn only_a_missing_firewall_rule_is_named_in_the_notice() {
    let m = Machine::set_up();
    let rules = m.sys.file("/etc/ufw/rules").unwrap();
    let kept: String = rules
        .lines()
        .filter(|l| !l.starts_with("5106/tcp"))
        .map(|l| format!("{l}\n"))
        .collect();
    m.sys.set_file("/etc/ufw/rules", &kept);
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(m.sys.prompted());
    let notice = position(&lines, |l| l == "needs administrator privileges for:");
    assert_eq!(
        lines[notice + 1],
        "  allow port 5106/tcp from 192.168.1.0/24 (flexai-combat)"
    );
    assert!(!lines[notice + 2].starts_with("  "), "{lines:#?}");
    let root: Vec<String> = m
        .sys
        .root_calls()
        .into_iter()
        .filter(|c| !c.starts_with("sudo -n "))
        .collect();
    assert_eq!(
        root,
        [
            "sudo ufw status",
            "sudo ufw allow from 192.168.1.0/24 to any port 5106 proto tcp comment local-machine hypostasis flexai-combat",
        ]
    );
}

#[test]
fn a_changed_network_replaces_our_stale_rule() {
    let m = Machine::set_up();
    let rules = m
        .sys
        .file("/etc/ufw/rules")
        .unwrap()
        .replace("5105/tcp ALLOW 192.168.1.0/24", "5105/tcp ALLOW 10.0.0.0/8");
    m.sys.set_file("/etc/ufw/rules", &rules);
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(
        m.sys.root_calls().contains(
            &"sudo ufw delete allow from 10.0.0.0/8 to any port 5105 proto tcp".to_string()
        ),
        "{:#?}",
        m.sys.root_calls()
    );
}

#[test]
fn existing_data_is_never_replaced_by_a_newer_backup() {
    let m = Machine::set_up();
    m.sys.set_file(
        &format!("{BACKUPS}/rpg-lib/rpg_library-2026-12-01.db"),
        "newer",
    );
    let (code, _) = setup(&m);
    assert_eq!(code, Ok(0));
    assert_eq!(m.sys.file(DB).as_deref(), Some("library"));
    assert!(m.calls_matching("copy ").is_empty());
    assert!(m.calls_matching("mp restore").is_empty());
}

#[test]
fn an_authority_that_differs_from_its_source_is_left_alone() {
    let m = Machine::set_up();
    let edited = format!("{}\n# hand edit\n", m.sys.file(AUTHORITY).unwrap());
    m.sys.set_file(AUTHORITY, &edited);
    let (code, _) = setup(&m);
    assert_eq!(code, Ok(0));
    assert_eq!(m.sys.file(AUTHORITY), Some(edited));
}

#[test]
fn running_services_are_never_stopped_or_restarted_by_setup() {
    let m = Machine::set_up();
    m.sys
        .respond_prefix("systemctl --user is-active", Output::ok("active\n"));
    // Change a unit so setup rewrites it while the service runs.
    let unit = format!("{UNIT_DIR}/local-machine-flexai-combat.service");
    m.sys.set_file(&unit, "old");
    let (code, lines) = setup(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(
        has_line(
            &lines,
            "note: flexai-combat: unit changed; run stop then start"
        ),
        "{lines:#?}"
    );
    for verb in ["stop", "restart", "disable", "start"] {
        assert!(
            m.calls_matching(&format!("systemctl --user {verb}"))
                .is_empty()
        );
    }
}
