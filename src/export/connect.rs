//! How clients reach the share: the local address inside `allowed_network`.

use std::net::IpAddr;

use crate::system::System;
use crate::system::net;

use super::config::ExportConfig;

/// The first local address inside `allowed_network`, parsed from `ip -o addr show`.
pub fn local_address(cfg: &ExportConfig, sys: &dyn System) -> Option<IpAddr> {
    net::local_address(&cfg.allowed_network, sys)
}

pub fn windows_path(cfg: &ExportConfig, addr: IpAddr) -> String {
    format!("\\\\{addr}\\{}", cfg.share_name)
}

pub fn unix_mount(cfg: &ExportConfig, addr: IpAddr) -> String {
    format!(
        "sudo mount -t cifs //{addr}/{share} /mnt/{share} -o username={owner},posix,uid=$(id -u),gid=$(id -g)",
        share = cfg.share_name,
        owner = cfg.owner,
    )
}

/// The `windows:` and `unix:` lines, or a note when no local address is in the network.
pub fn lines(cfg: &ExportConfig, addr: Option<IpAddr>) -> Vec<String> {
    match addr {
        Some(a) => vec![
            format!("windows: {}", windows_path(cfg, a)),
            format!("unix:    {}", unix_mount(cfg, a)),
        ],
        None => vec![format!(
            "warning: this machine has no address in {}; clients there cannot reach it",
            cfg.allowed_network
        )],
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::system::net::parse_addresses;

    #[test]
    fn parses_ip_output_and_formats_paths() {
        let out = "1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever\n\
                   2: enp0s3    inet 192.168.1.85/24 brd 192.168.1.255 scope global enp0s3\n\
                   3: enp0s8    inet 192.168.56.101/24 brd 192.168.56.255 scope global enp0s8\n";
        let addrs = parse_addresses(out);
        assert_eq!(addrs.len(), 3);
        let cfg = ExportConfig {
            allowed_network: "192.168.1.0/24".parse().unwrap(),
            share_name: "home".to_string(),
            path: PathBuf::from("/home/kostadis"),
            owner: "kostadis".to_string(),
            exclude: vec![],
        };
        let addr = addrs
            .into_iter()
            .find(|a| cfg.allowed_network.contains(a))
            .unwrap();
        assert_eq!(windows_path(&cfg, addr), r"\\192.168.1.85\home");
        assert_eq!(
            unix_mount(&cfg, addr),
            "sudo mount -t cifs //192.168.1.85/home /mnt/home -o username=kostadis,posix,uid=$(id -u),gid=$(id -g)"
        );
    }
}
