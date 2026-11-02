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
