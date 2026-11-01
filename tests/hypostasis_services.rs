//! `hypostasis start` / `stop` on a simulated machine (US3, T032). Never root, never a prompt.

mod support;

use local_machine::cli::HypostasisAction;
use local_machine::system::Output;
use support::*;

fn start(m: &Machine) -> (Result<u8, u8>, Vec<String>) {
    m.run(HypostasisAction::Start)
}

fn stop(m: &Machine) -> (Result<u8, u8>, Vec<String>) {
    m.run(HypostasisAction::Stop)
}

fn no_root(m: &Machine) {
    assert!(!m.sys.prompted());
    assert!(
        m.sys.calls().iter().all(|c| !c.starts_with("sudo")),
        "{:#?}",
        m.sys.calls()
    );
}

fn running() -> Machine {
    let m = Machine::set_up();
    m.services_start_cleanly();
    let (code, lines) = start(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    m.sys.clear_calls();
    m
}

#[test]
fn start_brings_up_all_three_once_they_answer() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    let (code, lines) = start(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert_eq!(m.calls_matching("systemctl --user enable --now").len(), 3);
    for (name, port) in SERVICES {
        assert!(
            has_line(
                &lines,
                &format!("changed: {name} (running at http://192.168.1.85:{port})")
            ),
            "{lines:#?}"
        );
    }
    // Reported only after the answer turned on: enable, then at least three probes.
    no_root(&m);
}

#[test]
fn not_ready_exits_4_and_touches_no_unit() {
    let m = Machine::fresh();
    let (code, lines) = start(&m);
    assert_eq!(code, Err(4), "{lines:#?}");
    assert!(m.calls_matching("systemctl --user").is_empty());
    no_root(&m);

    // Set up, but the combat workbook gone.
    let m = Machine::set_up();
    m.sys
        .files
        .borrow_mut()
        .remove(std::path::Path::new(&format!(
            "{COMBAT_DATA}/{}",
            COMBAT_FILES[0]
        )));
    let (code, lines) = start(&m);
    assert_eq!(code, Err(4), "{lines:#?}");
    let err = lines.last().unwrap();
    assert!(
        err.contains("not set up — run 'local-machine hypostasis setup'"),
        "{err}"
    );
    assert!(err.contains("not ready: flexai-combat (restore flexai-combat data: missing: FlexAI_Combat_2021_01_04.xlsx)"), "{err}");
    assert!(m.calls_matching("systemctl --user").is_empty());
}

#[test]
fn one_already_running_is_ok_and_not_enabled_again() {
    let m = running();
    let (code, lines) = start(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert!(
        has_line(&lines, "ok: rpg-lib (running at http://192.168.1.85:8000)"),
        "{lines:#?}"
    );
    assert!(m.calls_matching("enable").is_empty());
    no_root(&m);
}

#[test]
fn foreign_rpg_lib_is_left_alone_and_the_pair_still_starts() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    m.sys.set_http(8000, &[Some(200)]); // hand-run copy answering, unit inactive
    let (code, lines) = start(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        has_line(
            &lines,
            "error: rpg-lib: answering at http://192.168.1.85:8000 but not started by local-machine; not starting a second copy"
        ),
        "{lines:#?}"
    );
    assert!(
        m.calls_matching("enable --now local-machine-rpg-lib")
            .is_empty()
    );
    assert!(has_line(
        &lines,
        "changed: flexai-social (running at http://192.168.1.85:5105)"
    ));
    assert!(has_line(
        &lines,
        "changed: flexai-combat (running at http://192.168.1.85:5106)"
    ));
    no_root(&m);
}

#[test]
fn blocked_port_names_the_holder() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    m.sys.respond(
        "ss -ltnpH sport = :5106",
        Output::ok("LISTEN 0 5 0.0.0.0:5106 0.0.0.0:* users:((\"python3\",pid=42,fd=3))\n"),
    );
    let (code, lines) = start(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        has_line(&lines, "error: flexai-combat: port 5106 held by python3"),
        "{lines:#?}"
    );
    assert!(
        m.calls_matching("enable --now local-machine-flexai-combat")
            .is_empty()
    );
}

#[test]
fn timeout_disables_the_unit_and_shows_the_log() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    // rpg-lib never answers.
    m.sys.on_prefix(
        "systemctl --user enable --now local-machine-rpg-lib.service",
        |s, _| {
            s.set_http(8000, &[None]);
        },
    );
    let log: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    m.sys.set_file(&format!("{LOG_DIR}/rpg-lib.log"), &log);
    let (code, lines) = start(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert_eq!(
        m.calls_matching("disable --now local-machine-rpg-lib")
            .len(),
        1
    );
    let err = position(&lines, |l| {
        l == "error: rpg-lib: did not answer within 30 s"
    });
    assert_eq!(lines[err + 1], "  | line 11");
    assert_eq!(lines[err + 20], "  | line 30");
    assert_eq!(lines[err + 21], format!("  log: {LOG_DIR}/rpg-lib.log"));
    assert!(has_line(
        &lines,
        "changed: flexai-social (running at http://192.168.1.85:5105)"
    ));
    no_root(&m);
}

#[test]
fn stop_running_services_waits_for_silence() {
    let m = running();
    let (code, lines) = stop(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    assert_eq!(m.calls_matching("systemctl --user disable --now").len(), 3);
    for (name, _) in SERVICES {
        assert!(
            has_line(&lines, &format!("changed: {name} (stopped)")),
            "{lines:#?}"
        );
    }
    no_root(&m);
}

#[test]
fn stop_when_stopped_is_ok_and_calls_nothing() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    let (code, lines) = stop(&m);
    assert_eq!(code, Ok(0), "{lines:#?}");
    for (name, _) in SERVICES {
        assert!(
            has_line(&lines, &format!("ok: {name} (stopped)")),
            "{lines:#?}"
        );
    }
    assert!(m.calls_matching("disable").is_empty());
    no_root(&m);
}

#[test]
fn stop_disables_an_inactive_but_enabled_unit() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    m.sys
        .set_file("/run/enabled/local-machine-flexai-social.service", "");
    let (code, _) = stop(&m);
    assert_eq!(code, Ok(0));
    assert_eq!(
        m.calls_matching("systemctl --user disable"),
        ["systemctl --user disable local-machine-flexai-social.service"]
    );
}

#[test]
fn stop_never_touches_a_foreign_service() {
    let m = Machine::set_up();
    m.services_start_cleanly();
    m.sys.set_http(8000, &[Some(200)]);
    let (code, lines) = stop(&m);
    assert_eq!(code, Err(1), "{lines:#?}");
    assert!(
        has_line(
            &lines,
            "error: rpg-lib: answering but not started by local-machine; not stopping it"
        ),
        "{lines:#?}"
    );
    assert!(m.calls_matching("kill").is_empty());
    assert!(
        m.calls_matching("disable --now local-machine-rpg-lib")
            .is_empty()
    );
    no_root(&m);
}
