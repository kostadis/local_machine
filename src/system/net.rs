//! This machine's network addresses, shared by every subcommand.

use std::net::IpAddr;

use ipnet::IpNet;

use super::{Cmd, System};

/// Every address in `ip -o addr show`.
pub fn local_addresses(sys: &dyn System) -> Vec<IpAddr> {
    match sys.run(&Cmd::new("ip").args(["-o", "addr", "show"])) {
        Ok(out) => parse_addresses(&out.stdout),
        Err(_) => Vec::new(),
    }
}

/// The first local address inside `network`.
pub fn local_address(network: &IpNet, sys: &dyn System) -> Option<IpAddr> {
    local_addresses(sys)
        .into_iter()
        .find(|a| network.contains(a))
}

pub fn parse_addresses(stdout: &str) -> Vec<IpAddr> {
    let mut addrs = Vec::new();
    for line in stdout.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if let Some(pos) = tokens.iter().position(|t| *t == "inet" || *t == "inet6")
            && let Some(cidr) = tokens.get(pos + 1)
            && let Some(addr) = cidr.split('/').next()
            && let Ok(ip) = addr.parse()
        {
            addrs.push(ip);
        }
    }
    addrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ip_output() {
        let out = "1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever\n\
                   2: enp0s3    inet 192.168.1.85/24 brd 192.168.1.255 scope global enp0s3\n\
                   2: enp0s3    inet6 fe80::1/64 scope link\n";
        let addrs = parse_addresses(out);
        assert_eq!(addrs.len(), 3);
        assert_eq!(addrs[1].to_string(), "192.168.1.85");
    }
}
