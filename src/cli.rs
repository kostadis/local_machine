//! Command grammar: `local-machine <subcommand> <action>` (constitution Principle I).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "local-machine",
    version,
    about = "Tools for managing this machine"
)]
pub struct Cli {
    /// Configuration directory [default: ~/.config/local-machine]
    #[arg(long, global = true, value_name = "DIR")]
    pub config_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Share the home directory on the home network (Windows and Unix clients)
    #[command(arg_required_else_help = true)]
    Export {
        #[command(subcommand)]
        action: ExportAction,
    },
    /// Bring this machine to where `mneme up` works, and run its local services
    #[command(arg_required_else_help = true)]
    Hypostasis {
        #[command(subcommand)]
        action: HypostasisAction,
    },
}

#[derive(Subcommand, Debug, Clone, Copy)]
pub enum ExportAction {
    /// Install and configure everything sharing needs (needs sudo; re-runnable)
    Setup,
    /// Make the home directory available on the network, persistently
    Start,
    /// Report whether the share is available and how to connect
    Status,
    /// Make the share unavailable, persistently
    Stop,
}

#[derive(Subcommand, Debug, Clone, Copy)]
pub enum HypostasisAction {
    /// Install and configure everything mneme and the local services need (may need sudo;
    /// re-runnable; starts nothing)
    Setup,
    /// Start rpg-lib, flexai-social and flexai-combat, persistently (never prompts)
    Start,
    /// Report what is in place, what is missing, and what is running (never prompts)
    Status,
    /// Stop the local services, persistently (never prompts)
    Stop,
}
