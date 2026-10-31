//! `local-machine export`: share the home directory with Windows and Unix clients.

pub mod config;
pub mod connect;
pub mod samba;
pub mod setup;
pub mod start;
pub mod state;
pub mod status;
pub mod stop;
pub mod symlinks;

use std::path::Path;

use anyhow::Result;

use crate::cli::ExportAction;
use crate::system::System;

pub fn run(action: ExportAction, config_dir: &Path, sys: &dyn System) -> Result<u8> {
    let cfg = config::load(config_dir, sys)?;
    match action {
        ExportAction::Setup => setup::run(&cfg, sys),
        ExportAction::Start => start::run(&cfg, sys),
        ExportAction::Status => status::run(&cfg, sys),
        ExportAction::Stop => stop::run(&cfg, sys),
    }
}
