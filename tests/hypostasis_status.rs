//! `hypostasis status` (US4, T036): reports, never changes, never prompts.

mod support;

use local_machine::cli::HypostasisAction;
use local_machine::system::Output;
use support::*;

fn status(m: &Machine) -> (Result<u8, u8>, Vec<String>) {
    m.sys.clear_calls();
    let r = m.run(HypostasisAction::Status);
    assert!(!m.sys.prompted(), "status prompted");
    assert!(m.mutating_calls().is_empty(), "{:#?}", m.mutating_calls());
    r
}

fn running() -> Machine {
    let m = Machine::set_up();
    m.services_start_cleanly();
    assert_eq!(m.run(HypostasisAction::Start).0, Ok(0));
    m
}

const SEE_ALSO: &str =
    "see also: hypostasis status   (component pins, rendered configs, external services)";

#[test]
fn fully_set_up_and_running() {
    let m = running();
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert_eq!(lines[0], "hypostasis: set up; 3 of 3 services running");
    assert!(
        has_line(&lines, "running: rpg-lib        http://192.168.1.85:8000"),
        "{lines:#?}"
    );
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("running: ")).count(),
        3
    );
    assert_eq!(lines.last().unwrap(), SEE_ALSO);
}

#[test]
fn a_missing_checkout_is_exit_4() {
    let m = running();
    let p = "/home/kostadis/src/turbovecdb";
    m.sys.dirs.borrow_mut().retain(|d, _| !d.starts_with(p));
    m.sys.files.borrow_mut().retain(|d, _| !d.starts_with(p));
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(4), "{lines:#?}");
    assert!(
        has_line(
            &lines,
            &format!(
                "missing: checkout {p} ({p} is missing) — run 'local-machine hypostasis setup'"
            )
        ),
        "{lines:#?}"
    );
    assert!(lines[0].starts_with("hypostasis: not set up;"));
}

#[test]
fn a_stopped_service_is_exit_5() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(5), "{lines:#?}");
    assert!(
        has_line(
            &lines,
            "stopped: flexai-combat  http://192.168.1.85:5106 — run 'local-machine hypostasis start'"
        ),
        "{lines:#?}"
    );
    assert_eq!(lines.last().unwrap(), SEE_ALSO);
}

#[test]
fn uncached_sudo_makes_the_firewall_unknown_without_failing() {
    let m = running();
    m.sys.respond_fn("sudo -n ufw status", |_, _| {
        Output::fail(1, "sudo: a password is required")
    });
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(
        has_line(
            &lines,
            "unknown: firewall rpg-lib (run 'sudo -v' first to check)"
        ),
        "{lines:#?}"
    );
}

#[test]
fn a_failed_unit_shows_its_log() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    m.sys.respond(
        "systemctl --user is-failed local-machine-rpg-lib.service",
        Output::ok("failed\n"),
    );
    m.sys.set_file(
        &format!("{LOG_DIR}/rpg-lib.log"),
        "Traceback\nsqlite3.OperationalError\n",
    );
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(5), "{lines:#?}");
    let at = position(&lines, |l| l.starts_with("failed:  rpg-lib"));
    assert_eq!(lines[at + 1], "  | Traceback");
    assert_eq!(lines[at + 3], format!("  log: {LOG_DIR}/rpg-lib.log"));
}

#[test]
fn a_foreign_service_is_reported() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    m.sys.set_http(8000, &[Some(200)]);
    let (_, lines) = status(&m);
    assert!(
        lines.iter().any(|l| l.starts_with("foreign: rpg-lib")),
        "{lines:#?}"
    );
}

#[test]
fn an_unmounted_backup_words_the_fix() {
    let m = Machine::set_up();
    m.sys.files.borrow_mut().remove(std::path::Path::new(DB));
    m.unmount_backups();
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(4), "{lines:#?}");
    assert!(
        has_line(
            &lines,
            &format!(
                "missing: restore rpg-library ({DB} does not exist) — mount {BACKUPS}/rpg-lib, then run 'local-machine hypostasis setup'"
            )
        ),
        "{lines:#?}"
    );
}

#[test]
fn fresh_machine_reports_not_set_up() {
    let m = Machine::fresh();
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(4), "{lines:#?}");
    assert_eq!(lines[0], "hypostasis: not set up; 0 of 3 services running");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("missing: venv (needs the authority file")),
        "{lines:#?}"
    );
    assert_eq!(lines.last().unwrap(), SEE_ALSO);
}

#[test]
fn root_is_exit_3() {
    let m = Machine::set_up();
    m.sys.set_euid(0);
    assert_eq!(m.run(HypostasisAction::Status).0, Err(3));
}

#[test]
fn flexai_data_waits_for_the_services_venv_instead_of_blaming_the_backup() {
    let m = Machine::set_up();
    m.unmount_backups();
    m.sys.dirs.borrow_mut().retain(|d, _| !d.starts_with(SVENV));
    m.sys
        .files
        .borrow_mut()
        .retain(|d, _| !d.starts_with(SVENV));
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(4), "{lines:#?}");
    for app in ["flexai-social", "flexai-combat"] {
        assert!(
            has_line(
                &lines,
                &format!(
                    "missing: restore {app} data (checked once setup has made {SVENV}) — run \
                     'local-machine hypostasis setup'"
                )
            ),
            "{lines:#?}"
        );
    }
}

// ---------------------------------------------------------------- the library palace (003 US2)

const LIB_STATUS: &str = "/home/kostadis/.venvs/main/bin/library status --json";
const AUTH: &str = "/home/kostadis/.config/hypostasis/hypostasis.yaml";

/// Only `library status --json` may reach the library binary, and it is reported once.
fn only_status_call(m: &Machine) {
    for c in m.calls_matching("/bin/library") {
        assert_eq!(c, LIB_STATUS, "{:#?}", m.sys.calls());
    }
    assert!(!m.sys.prompted());
}

fn library_lines(lines: &[String]) -> Vec<&String> {
    lines
        .iter()
        .filter(|l| l.contains("library palace") && !l.starts_with("hypostasis:"))
        .collect()
}

#[test]
fn library_ready_is_one_ok_line() {
    let m = running();
    let out = Output::ok(r#"{"books": 2, "stale": [], "index_bytes": 46000000}"#);
    m.sys.respond_fn(LIB_STATUS, move |_, _| out.clone());
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert_eq!(
        library_lines(&lines),
        vec!["ok: library palace (2 books, 46 MB)"]
    );
    assert_eq!(lines[0], "hypostasis: set up; 3 of 3 services running");
    only_status_call(&m);
}

#[test]
fn library_ready_with_stale_notes_refresh() {
    let m = running();
    let out = Output::ok(r#"{"books": 2, "stale": ["a","b","c"], "index_bytes": 46000000}"#);
    m.sys.respond_fn(LIB_STATUS, move |_, _| out.clone());
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert_eq!(
        library_lines(&lines),
        vec![
            "ok: library palace (2 books, 46 MB)",
            "note: library palace: 3 books stale — run '/home/kostadis/.venvs/main/bin/library refresh'"
        ]
    );
    only_status_call(&m);
}

#[test]
fn library_not_ready_states_map_to_exit_codes() {
    let live = format!(
        "error: library palace: overlaps a live palace (inside live) — change library.palace in {AUTH}"
    );
    let cases: [(u8, &str, u8, &str); 4] = [
        (
            3,
            "no config",
            3,
            "missing: library palace (not configured: no config)",
        ),
        (
            4,
            "library not built",
            4,
            "missing: library palace (not built) — run 'local-machine hypostasis setup'",
        ),
        (
            5,
            "model differs",
            1,
            "error: library palace: embedder mismatch (model differs)",
        ),
        (6, "inside live", 1, &live),
    ];
    for (lib_code, msg, exit, line) in cases {
        let m = running();
        let out = Output::fail(lib_code as i32, msg);
        m.sys.respond_fn(LIB_STATUS, move |_, _| out.clone());
        let (code, lines) = status(&m);
        assert_eq!(code, Ok(exit), "{lib_code}: {lines:#?}");
        assert_eq!(library_lines(&lines), vec![line], "{lib_code}");
        assert_eq!(
            lines[0],
            "hypostasis: set up; library palace not ready; 3 of 3 services running"
        );
        only_status_call(&m);
    }
}

#[test]
fn library_other_failure_is_exit_1() {
    let m = running();
    let out = Output::fail(7, "kaboom");
    m.sys.respond_fn(LIB_STATUS, move |_, _| out.clone());
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(1), "{lines:#?}");
    assert_eq!(
        library_lines(&lines),
        vec!["error: library palace: library status failed: kaboom"]
    );
    only_status_call(&m);
}

#[test]
fn library_other_piece_missing_outranks_library_problem() {
    let m = running();
    let p = "/home/kostadis/src/turbovecdb";
    m.sys.dirs.borrow_mut().retain(|d, _| !d.starts_with(p));
    m.sys.files.borrow_mut().retain(|d, _| !d.starts_with(p));
    let out = Output::fail(3, "no config");
    m.sys.respond_fn(LIB_STATUS, move |_, _| out.clone());
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(4), "{lines:#?}");
    assert!(
        lines[0].starts_with("hypostasis: not set up;"),
        "{lines:#?}"
    );
    only_status_call(&m);
}

#[test]
fn library_mismatch_outranks_stopped_services_and_ready_does_not() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    let out = Output::fail(5, "model differs");
    m.sys.respond_fn(LIB_STATUS, move |_, _| out.clone());
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(1), "{lines:#?}");
    only_status_call(&m);

    let m = Machine::set_up();
    m.services_start_cleanly();
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(5), "{lines:#?}");
    assert_eq!(lines[0], "hypostasis: set up; 0 of 3 services running");
    only_status_call(&m);
}

#[test]
fn library_missing_command_is_reported_without_running_it() {
    let m = running();
    m.sys
        .files
        .borrow_mut()
        .remove(std::path::Path::new(LIB_BIN));
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(4), "{lines:#?}");
    assert_eq!(
        library_lines(&lines),
        vec![
            "missing: library palace (needs the library command from CampaignGenerator) — run 'local-machine hypostasis setup'"
        ]
    );
    assert!(m.calls_matching("/bin/library").is_empty());
}

#[test]
fn library_without_authority_makes_no_library_call() {
    let m = running();
    m.sys.files.borrow_mut().remove(std::path::Path::new(AUTH));
    let (code, lines) = status(&m);
    assert_eq!(code, Ok(4), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("missing: restores (needs the authority file")),
        "{lines:#?}"
    );
    assert!(library_lines(&lines).is_empty(), "{lines:#?}");
    assert!(m.calls_matching("/bin/library").is_empty());
}
