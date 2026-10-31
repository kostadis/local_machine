//! `local-machine`: tools for managing this machine.

pub mod cli;
pub mod config;
pub mod export;
pub mod system;

use std::fmt;

use cli::{Cli, Command};
use system::System;

/// An error that ends the program with a specific exit code (contracts/cli.md).
#[derive(Debug)]
pub struct ExitError {
    pub code: u8,
    pub message: String,
}

impl ExitError {
    pub const FAILED: u8 = 1;
    pub const NOT_CONFIGURED: u8 = 3;
    pub const NOT_SET_UP: u8 = 4;
    pub const NOT_AVAILABLE: u8 = 5;

    pub fn new(code: u8, message: impl Into<String>) -> Self {
        ExitError {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ExitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExitError {}

/// Run a parsed command. Returns the exit code for a normal finish.
pub fn run(cli: &Cli, sys: &dyn System) -> anyhow::Result<u8> {
    let config_dir = config::resolve_dir(cli.config_dir.as_deref());
    match &cli.command {
        Command::Export { action } => export::run(*action, &config_dir, sys),
    }
}

/// Exit code for an error returned by [`run`].
pub fn exit_code(err: &anyhow::Error) -> u8 {
    if let Some(e) = err.downcast_ref::<ExitError>() {
        e.code
    } else if err.downcast_ref::<config::ConfigError>().is_some() {
        ExitError::NOT_CONFIGURED
    } else {
        ExitError::FAILED
    }
}
