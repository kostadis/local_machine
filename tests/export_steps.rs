//! `export` actions against a fake system: idempotence, rollback, privileges, exit codes.

use local_machine::cli::ExportAction;
use local_machine::export::{self, config, samba, state};
use local_machine::system::Output;
use local_machine::system::fake::FakeSystem;
use local_machine::{ExitError, exit_code};

const UBUNTU_SMB_CONF: &str = "[global]\n   workgroup = WORKGROUP\n[printers]\n   path = /var/tmp\n\
                               [print$]\n   path = /var/lib/samba/printers\n";

struct Fixture {
    config_dir: tempfile::TempDir,
    home: tempfile::TempDir,
    sys: FakeSystem,
}

impl Fixture {
    fn new() -> Self {
        let config_dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let sys = FakeSystem::new("kostadis");
        sys.respond(
            "getent passwd kostadis",
            Output::ok("kostadis:x:1000:1000::/home/kostadis:/bin/bash\n"),
        );
        sys.set_dir(home.path().to_str().unwrap(), 1000);
        sys.set_file(samba::SMB_CONF, UBUNTU_SMB_CONF);
        sys.respond(
            "ip -o addr show",
            Output::ok("2: enp0s3    inet 192.168.1.85/24 brd 192.168.1.255 scope global enp0s3\n"),
        );
        let f = Fixture {
            config_dir,
            home,
            sys,
        };
        f.write_config("192.168.1.0/24");
        f
    }

    fn write_config(&self, network: &str) {
        std::fs::write(
            self.config_dir.path().join("export.yaml"),
            format!(
                "allowed_network: {network}\npath: {}\n",
                self.home.path().display()
            ),
        )
        .unwrap();
    }

    fn run(&self, action: ExportAction) -> Result<u8, u8> {
        export::run(action, self.config_dir.path(), &self.sys).map_err(|e| exit_code(&e))
    }

    /// What the fake reports once `export setup` has really happened.
    fn mark_installed(&self) {
        self.sys.respond(
            "dpkg-query -W -f=${Status} samba",
            Output::ok("install ok installed"),
        );
        self.sys
            .respond("sudo -n pdbedit -L", Output::ok("kostadis:1000:\n"));
    }

    fn mark_smbd(&self, running: bool) {
        let (enabled, active) = if running {
            ("enabled", "active")
        } else {
            ("disabled", "inactive")
        };
        self.sys
            .respond("systemctl is-enabled smbd", Output::ok(enabled));
        self.sys
            .respond("systemctl is-active smbd", Output::ok(active));
    }

    fn setup(&self) {
        self.sys.push_password("pw");
        self.sys.push_password("pw");
        assert_eq!(self.run(ExportAction::Setup), Ok(0));
        self.mark_installed();
        self.sys.clear_calls();
    }

    fn cfg(&self) -> config::ExportConfig {
        config::load(self.config_dir.path(), &self.sys).unwrap()
    }
}

fn position(calls: &[String], line: &str) -> usize {
    calls
        .iter()
        .position(|c| c == line)
        .unwrap_or_else(|| panic!("`{line}` not called; calls: {calls:#?}"))
}

#[test]
fn setup_on_a_fresh_machine() {
    let f = Fixture::new();
    f.sys.push_password("pw");
    f.sys.push_password("pw");
    assert_eq!(f.run(ExportAction::Setup), Ok(0));

    let calls = f.sys.calls();
    let first_root = calls.iter().position(|c| c.starts_with("sudo ")).unwrap();
    assert_eq!(
        calls[first_root], "sudo -v",
        "privilege notice comes before any root command"
    );
    position(
        &calls,
        "sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y samba",
    );
    // smbd was not enabled before the install, so setup must not leave it running.
    position(&calls, "sudo systemctl disable --now smbd");
    position(&calls, "sudo smbpasswd -s -a kostadis");
    assert!(!calls.iter().any(|c| c.contains("systemctl enable")));

    let globals = f.sys.file(samba::GLOBALS_PATH).unwrap();
    assert!(globals.starts_with(&format!("{}[global]\n", samba::HEADER)));
    let smb_conf = f.sys.file(samba::SMB_CONF).unwrap();
    assert_eq!(smb_conf.lines().last().unwrap(), samba::include_line());
    assert_eq!(
        f.sys.file(samba::SHARE_PATH).unwrap(),
        samba::render_empty_share()
    );
}

#[test]
fn setup_twice_changes_nothing_the_second_time() {
    let f = Fixture::new();
    f.setup();
    assert_eq!(f.run(ExportAction::Setup), Ok(0));
    let calls = f.sys.calls();
    assert!(!calls.iter().any(|c| c.contains("apt-get")));
    assert!(!calls.iter().any(|c| c.contains("smbpasswd")));
    assert!(!calls.iter().any(|c| c.starts_with("install ")));
}

#[test]
fn setup_rejects_mismatched_passwords_and_rolls_back() {
    let f = Fixture::new();
    f.sys.push_password("one");
    f.sys.push_password("two");
    assert_eq!(f.run(ExportAction::Setup), Err(ExitError::FAILED));
    assert!(!f.sys.calls().iter().any(|c| c.contains("smbpasswd")));
    assert_eq!(f.sys.file(samba::GLOBALS_PATH), None, "globals rolled back");
    assert_eq!(
        f.sys.file(samba::SHARE_PATH),
        None,
        "share file rolled back"
    );
    assert_eq!(
        f.sys.file(samba::SMB_CONF).unwrap(),
        UBUNTU_SMB_CONF,
        "include rolled back"
    );
}

#[test]
fn start_before_setup_refuses_and_changes_nothing() {
    let f = Fixture::new();
    assert_eq!(f.run(ExportAction::Start), Err(ExitError::NOT_SET_UP));
    assert!(f.sys.root_calls().iter().all(|c| c.starts_with("sudo -n ")));
    assert!(!f.sys.calls().iter().any(|c| c.starts_with("install ")));
}

#[test]
fn start_makes_the_share_available_once() {
    let f = Fixture::new();
    f.setup();
    assert_eq!(f.run(ExportAction::Start), Ok(0));
    let calls = f.sys.calls();
    assert_eq!(
        calls.iter().find(|c| c.starts_with("sudo ")).unwrap(),
        "sudo -v"
    );
    position(&calls, "sudo systemctl enable --now smbd");
    assert_eq!(
        f.sys.file(samba::SHARE_PATH).unwrap(),
        samba::render_share(&f.cfg())
    );

    f.mark_smbd(true);
    f.sys.clear_calls();
    assert_eq!(f.run(ExportAction::Start), Ok(0));
    assert!(!f.sys.calls().iter().any(|c| c.contains("systemctl enable")));
    assert!(!f.sys.calls().iter().any(|c| c.starts_with("install ")));
}

#[test]
fn start_restores_the_share_file_when_testparm_fails() {
    let f = Fixture::new();
    f.setup();
    f.sys.respond("testparm -s", Output::fail(1, "bad config"));
    assert_eq!(f.run(ExportAction::Start), Err(ExitError::FAILED));
    assert_eq!(
        f.sys.file(samba::SHARE_PATH).unwrap(),
        samba::render_empty_share()
    );
    assert!(!f.sys.calls().iter().any(|c| c.contains("systemctl enable")));
}

#[test]
fn network_change_requires_setup_which_replaces_the_firewall_rule() {
    let f = Fixture::new();
    f.setup();
    assert_eq!(f.run(ExportAction::Start), Ok(0));
    f.mark_smbd(true);

    f.write_config("10.0.0.0/24");
    let report = state::detect(&f.cfg(), &f.sys);
    assert!(report.config_changed);
    assert_eq!(f.run(ExportAction::Start), Err(ExitError::NOT_SET_UP));

    let ufw = "Status: active\n\nTo     Action   From\n--     ------   ----\n\
               Samba  ALLOW    192.168.1.0/24\n";
    f.sys.respond("sudo -n ufw status", Output::ok(ufw));
    f.sys.respond("sudo ufw status", Output::ok(ufw));
    f.sys.clear_calls();
    assert_eq!(f.run(ExportAction::Setup), Ok(0));
    let calls = f.sys.calls();
    let delete = position(
        &calls,
        "sudo ufw delete allow from 192.168.1.0/24 to any app Samba",
    );
    let add = position(&calls, "sudo ufw allow from 10.0.0.0/24 to any app Samba");
    assert!(delete < add);
    // The share was running: new globals are reloaded, availability unchanged.
    position(&calls, "sudo smbcontrol smbd reload-config");
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("systemctl disable") || c.contains("systemctl enable"))
    );
}

#[test]
fn status_exit_codes_and_no_prompts() {
    let f = Fixture::new();
    assert_eq!(f.run(ExportAction::Status), Ok(ExitError::NOT_SET_UP));

    f.setup();
    assert_eq!(f.run(ExportAction::Status), Ok(ExitError::NOT_AVAILABLE));

    assert_eq!(f.run(ExportAction::Start), Ok(0));
    f.mark_smbd(true);
    f.sys.clear_calls();
    assert_eq!(f.run(ExportAction::Status), Ok(0));
    let calls = f.sys.calls();
    assert!(!calls.contains(&"sudo -v".to_string()));
    assert!(f.sys.root_calls().iter().all(|c| c.starts_with("sudo -n ")));

    std::fs::remove_file(f.config_dir.path().join("export.yaml")).unwrap();
    assert_eq!(f.run(ExportAction::Status), Err(ExitError::NOT_CONFIGURED));
}

#[test]
fn status_reports_unknown_when_sudo_refuses() {
    let f = Fixture::new();
    f.setup();
    assert_eq!(f.run(ExportAction::Start), Ok(0));
    f.mark_smbd(true);
    f.sys.respond(
        "sudo -n pdbedit -L",
        Output::fail(1, "a password is required"),
    );
    f.sys.respond(
        "sudo -n ufw status",
        Output::fail(1, "a password is required"),
    );
    let report = state::detect(&f.cfg(), &f.sys);
    assert_eq!(report.unknown, vec![state::SAMBA_USER, state::FIREWALL]);
    assert!(report.missing.is_empty());
    assert_eq!(f.run(ExportAction::Status), Ok(0));
}

#[test]
fn status_detects_share_drift() {
    let f = Fixture::new();
    f.setup();
    assert_eq!(f.run(ExportAction::Start), Ok(0));
    std::fs::write(
        f.config_dir.path().join("export.yaml"),
        format!(
            "allowed_network: 192.168.1.0/24\npath: {}\nexclude: [.ssh]\n",
            f.home.path().display()
        ),
    )
    .unwrap();
    let report = state::detect(&f.cfg(), &f.sys);
    assert!(report.share_drift);
    assert!(
        !report.config_changed,
        "exclude is share-level: start applies it"
    );
}

#[test]
fn stop_then_stop_again() {
    let f = Fixture::new();
    f.setup();
    assert_eq!(f.run(ExportAction::Start), Ok(0));
    f.mark_smbd(true);
    f.sys.clear_calls();

    assert_eq!(f.run(ExportAction::Stop), Ok(0));
    let calls = f.sys.calls();
    position(&calls, "sudo smbcontrol smbd reload-config");
    position(&calls, "sudo systemctl disable --now smbd");
    assert_eq!(
        f.sys.file(samba::SHARE_PATH).unwrap(),
        samba::render_empty_share()
    );

    f.mark_smbd(false);
    f.sys.clear_calls();
    assert_eq!(f.run(ExportAction::Stop), Ok(0));
    assert!(f.sys.root_calls().iter().all(|c| c.starts_with("sudo -n ")));
}

#[test]
fn stop_before_setup_refuses_without_installing() {
    let f = Fixture::new();
    assert_eq!(f.run(ExportAction::Stop), Err(ExitError::NOT_SET_UP));
    assert!(!f.sys.calls().iter().any(|c| c.contains("apt-get")));
}

#[test]
fn password_never_reaches_a_command_line() {
    let f = Fixture::new();
    f.sys.push_password("s3cret-pw");
    f.sys.push_password("s3cret-pw");
    assert_eq!(f.run(ExportAction::Setup), Ok(0));
    assert!(!f.sys.calls().iter().any(|c| c.contains("s3cret-pw")));
    let config = std::fs::read_to_string(f.config_dir.path().join("export.yaml")).unwrap();
    assert!(!config.contains("s3cret-pw"));
}
