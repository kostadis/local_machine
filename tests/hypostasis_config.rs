//! The owner gate shared by every `hypostasis` action (T012): exit 3, no sudo, no change.

mod support;

use local_machine::cli::HypostasisAction;
use support::Machine;

const ACTIONS: [HypostasisAction; 4] = [
    HypostasisAction::Setup,
    HypostasisAction::Start,
    HypostasisAction::Status,
    HypostasisAction::Stop,
];

fn refused(m: &Machine, needle: &str) {
    for action in ACTIONS {
        m.sys.clear_calls();
        let (code, lines) = m.run(action);
        assert_eq!(code, Err(3), "{action:?}: {lines:#?}");
        assert!(
            lines.iter().any(|l| l.contains(needle)),
            "{action:?}: expected {needle:?} in {lines:#?}"
        );
        assert!(
            m.sys.calls().iter().all(|c| !c.starts_with("sudo")),
            "{action:?} ran sudo: {:#?}",
            m.sys.calls()
        );
        assert!(m.mutating_calls().is_empty(), "{:#?}", m.mutating_calls());
    }
}

#[test]
fn missing_machine_yaml() {
    let m = Machine::fresh();
    std::fs::remove_file(m.config_dir.path().join("machine.yaml")).unwrap();
    refused(&m, "machine.yaml (need at least allowed_network)");
}

#[test]
fn missing_hypostasis_yaml() {
    let m = Machine::fresh();
    std::fs::remove_file(m.config_dir.path().join("hypostasis.yaml")).unwrap();
    refused(&m, "hypostasis.yaml (need at least authority_source");
}

#[test]
fn run_as_root() {
    let m = Machine::fresh();
    m.sys.set_euid(0);
    refused(&m, "must not run as root");
}

#[test]
fn wrong_user() {
    let m = Machine::fresh();
    std::fs::write(
        m.config_dir.path().join("machine.yaml"),
        "allowed_network: 192.168.1.0/24\nowner: alice\n",
    )
    .unwrap();
    m.sys.respond(
        "getent passwd alice",
        local_machine::system::Output::ok("alice:x:1001:1001::/home/alice:/bin/bash\n"),
    );
    refused(&m, "run as alice (the owner in");
}

#[test]
fn invalid_authority_is_exit_3_naming_it() {
    let m = Machine::set_up();
    m.sys.set_file(support::AUTHORITY, "installer: pip\n");
    let (code, lines) = m.run(HypostasisAction::Status);
    assert_eq!(code, Err(3), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains(support::AUTHORITY) && l.contains("hypostasis's authority file")),
        "{lines:#?}"
    );
}
