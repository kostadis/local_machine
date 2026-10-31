//! Command shape and exit codes (contracts/cli.md).

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("local-machine").unwrap()
}

#[test]
fn export_without_action_shows_help_and_exits_2() {
    bin()
        .arg("export")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("setup"))
        .stderr(predicate::str::contains("start"))
        .stderr(predicate::str::contains("status"))
        .stderr(predicate::str::contains("stop"));
}

#[test]
fn unknown_action_exits_2() {
    bin().args(["export", "bogus"]).assert().code(2);
}

#[test]
fn config_dir_is_accepted_before_and_after_the_subcommand() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().to_str().unwrap();
    bin()
        .args(["--config-dir", d, "export", "status"])
        .assert()
        .code(3);
    bin()
        .args(["export", "status", "--config-dir", d])
        .assert()
        .code(3);
}

#[test]
fn every_action_without_config_exits_3_naming_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let expected = format!("no config at {}/export.yaml", dir.path().display());
    for action in ["setup", "start", "status", "stop"] {
        bin()
            .args(["export", action, "--config-dir"])
            .arg(dir.path())
            .assert()
            .code(3)
            .stderr(predicate::str::contains(expected.clone()));
    }
}

#[test]
fn default_config_dir_is_under_home() {
    let home = tempfile::tempdir().unwrap();
    let expected = format!(
        "no config at {}/.config/local-machine/export.yaml",
        home.path().display()
    );
    bin()
        .args(["export", "status"])
        .env("HOME", home.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains(expected));
}
