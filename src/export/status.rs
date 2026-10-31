//! `local-machine export status`: report without changing anything or prompting (FR-007).

use anyhow::Result;

use crate::ExitError;
use crate::system::System;

use super::config::ExportConfig;
use super::state::{self, ShareState};
use super::{connect, symlinks};

pub fn run(cfg: &ExportConfig, sys: &dyn System) -> Result<u8> {
    let report = state::detect(cfg, sys);
    let (headline, code) = match report.state {
        ShareState::Available => ("export: available".to_string(), 0),
        ShareState::SetUp => (
            "export: set up, not available".to_string(),
            ExitError::NOT_AVAILABLE,
        ),
        ShareState::NotSetUp => (report.not_set_up_message(), ExitError::NOT_SET_UP),
    };
    println!("{headline}");

    let excluding = if cfg.exclude.is_empty() {
        String::new()
    } else {
        format!("  (excluding {})", cfg.exclude.join(", "))
    };
    println!("shared:  {}{excluding}", cfg.path.display());

    let addr = connect::local_address(cfg, sys);
    let reach = match addr {
        Some(a) => format!("reachable: yes, via {a}"),
        None => "reachable: no".to_string(),
    };
    println!("network: {}  ({reach})", cfg.allowed_network);
    for line in connect::lines(cfg, addr) {
        println!("{line}");
    }

    for (name, note) in &report.notes {
        println!("note: {name}: {note}");
    }
    for name in &report.unknown {
        println!(
            "warning: could not check {name} without sudo; run 'sudo -v' first for a full report"
        );
    }
    if report.share_drift && report.state != ShareState::NotSetUp {
        println!(
            "warning: the share differs from the configuration; run 'local-machine export start'"
        );
    }

    let scan = symlinks::scan(&cfg.path, &cfg.exclude);
    for (link, target) in &scan.warnings {
        println!(
            "warning: symlink {} -> {} leads into an excluded name",
            link.display(),
            target.display()
        );
    }
    if scan.depth_limit_hit {
        println!(
            "note: symlink scan stopped {} levels below {}",
            symlinks::MAX_DEPTH,
            cfg.path.display()
        );
    }
    Ok(code)
}
