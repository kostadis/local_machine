//! Rendering of the two managed Samba files (research R2–R8). Pure: no I/O.

use super::config::ExportConfig;

pub const SMB_CONF: &str = "/etc/samba/smb.conf";
pub const GLOBALS_PATH: &str = "/etc/samba/local-machine-export.conf";
pub const SHARE_PATH: &str = "/etc/samba/local-machine-export-share.conf";
pub const HEADER: &str = "# Managed by local-machine export — do not edit\n";

pub fn include_line() -> String {
    format!("include = {GLOBALS_PATH}")
}

/// Global settings. Starts with `[global]` because the include is appended after the last
/// share (`[print$]`) in Ubuntu's smb.conf, and Samba applies an include to the section it
/// appears in.
pub fn render_globals(cfg: &ExportConfig) -> String {
    let settings = [
        ("map to guest", "never".to_string()),
        ("restrict anonymous", "2".to_string()),
        ("server min protocol", "SMB3".to_string()),
        ("disable netbios", "yes".to_string()),
        ("hosts allow", format!("{} 127.0.0.1", cfg.allowed_network)),
        ("hosts deny", "ALL".to_string()),
        ("follow symlinks", "yes".to_string()),
        ("wide links", "no".to_string()),
        ("allow insecure wide links", "no".to_string()),
        ("load printers", "no".to_string()),
        ("printing", "bsd".to_string()),
        ("printcap name", "/dev/null".to_string()),
        ("disable spoolss", "yes".to_string()),
        ("include", SHARE_PATH.to_string()),
    ];
    let mut out = String::from(HEADER);
    out.push_str("[global]\n");
    for (key, value) in settings {
        out.push_str(&format!("   {key} = {value}\n"));
    }
    out
}

/// The share definition written by `export start`.
pub fn render_share(cfg: &ExportConfig) -> String {
    let mut out = String::from(HEADER);
    out.push_str(&format!("[{}]\n", cfg.share_name));
    let mut settings = vec![
        ("path", cfg.path.display().to_string()),
        ("valid users", cfg.owner.clone()),
        ("force user", cfg.owner.clone()),
        ("read only", "no".to_string()),
        ("browseable", "yes".to_string()),
        ("smb3 unix extensions", "yes".to_string()),
    ];
    if !cfg.exclude.is_empty() {
        settings.push(("veto files", format!("/{}/", cfg.exclude.join("/"))));
        settings.push(("delete veto files", "no".to_string()));
    }
    for (key, value) in settings {
        out.push_str(&format!("   {key} = {value}\n"));
    }
    out
}

/// The share file contents while nothing is shared (after setup or stop).
pub fn render_empty_share() -> String {
    HEADER.to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn cfg(exclude: &[&str]) -> ExportConfig {
        ExportConfig {
            allowed_network: "192.168.1.0/24".parse().unwrap(),
            share_name: "home".to_string(),
            path: PathBuf::from("/home/kostadis"),
            owner: "kostadis".to_string(),
            exclude: exclude.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn globals_exact() {
        assert_eq!(
            render_globals(&cfg(&[".ssh", ".gnupg"])),
            "# Managed by local-machine export — do not edit\n\
             [global]\n   map to guest = never\n   restrict anonymous = 2\n\
             \x20  server min protocol = SMB3\n   disable netbios = yes\n\
             \x20  hosts allow = 192.168.1.0/24 127.0.0.1\n   hosts deny = ALL\n\
             \x20  follow symlinks = yes\n   wide links = no\n   allow insecure wide links = no\n\
             \x20  load printers = no\n   printing = bsd\n   printcap name = /dev/null\n\
             \x20  disable spoolss = yes\n\
             \x20  include = /etc/samba/local-machine-export-share.conf\n"
        );
    }

    #[test]
    fn globals_open_with_global_section() {
        let text = render_globals(&cfg(&[]));
        let first = text.lines().find(|l| !l.starts_with('#')).unwrap();
        assert_eq!(first, "[global]");
    }

    #[test]
    fn share_exact_with_default_excludes() {
        assert_eq!(
            render_share(&cfg(&[".ssh", ".gnupg"])),
            "# Managed by local-machine export — do not edit\n[home]\n\
             \x20  path = /home/kostadis\n   valid users = kostadis\n   force user = kostadis\n\
             \x20  read only = no\n   browseable = yes\n   smb3 unix extensions = yes\n\
             \x20  veto files = /.ssh/.gnupg/\n   delete veto files = no\n"
        );
    }

    #[test]
    fn share_without_excludes_has_no_veto() {
        assert!(!render_share(&cfg(&[])).contains("veto"));
    }

    #[test]
    fn unix_extensions_on_and_permissions_never_rewritten() {
        let share = render_share(&cfg(&[".ssh"]));
        let globals = render_globals(&cfg(&[".ssh"]));
        assert!(share.contains("smb3 unix extensions = yes"));
        for text in [&share, &globals] {
            for forbidden in [
                "create mask",
                "force create mode",
                "directory mask",
                "force directory mode",
            ] {
                assert!(
                    !text.contains(forbidden),
                    "{forbidden} must not be set (FR-017)"
                );
            }
        }
    }

    #[test]
    fn change_notify_never_disabled() {
        for text in [render_share(&cfg(&[])), render_globals(&cfg(&[]))] {
            assert!(!text.contains("change notify = no"), "FR-013");
        }
    }
}
