//! `local-machine hypostasis`: bring this machine to where `mneme up` works, and run the
//! local services (specs/002-hypostasis-setup).

pub mod authority;
pub mod checkout;
pub mod config;
pub mod firewall;
pub mod pieces;
pub mod restore;
pub mod services;
pub mod setup;
pub mod start;
pub mod status;
pub mod stop;

use std::path::Path;

use anyhow::Result;

use crate::ExitError;
use crate::cli::HypostasisAction;
use crate::config::ConfigError;
use crate::machine::{self, MachineConfig};
use crate::system::System;

use self::authority::Authority;
use self::config::HypostasisConfig;

/// Everything an action needs once the owner gate has passed.
#[derive(Clone, Debug)]
pub struct Context {
    pub machine: MachineConfig,
    pub cfg: HypostasisConfig,
}

impl Context {
    pub fn home(&self) -> &Path {
        &self.machine.owner_home
    }

    /// hypostasis's authority file, read fresh each time (setup seeds it partway through).
    /// `Ok(None)` while it does not exist.
    pub fn authority(&self, sys: &dyn System) -> Result<Option<Authority>, ConfigError> {
        authority::load_if_present(sys, &self.cfg.authority, self.home())
    }
}

/// The owner gate, shared by all four actions: not root, the configured owner, both config
/// files valid. Refuses with exit 3 before anything changes or prompts.
pub fn context(config_dir: &Path, sys: &dyn System) -> Result<Context> {
    if sys.effective_uid() == 0 {
        return Err(ExitError::new(
            ExitError::NOT_CONFIGURED,
            "hypostasis: must not run as root; run it as the owner",
        )
        .into());
    }
    let machine = machine::load(config_dir, sys)?;
    let user = sys.current_user().unwrap_or_default();
    if user != machine.owner {
        return Err(ExitError::new(
            ExitError::NOT_CONFIGURED,
            format!(
                "hypostasis: run as {} (the owner in {}), not {}",
                machine.owner,
                machine::path(config_dir).display(),
                if user.is_empty() {
                    "an unknown user"
                } else {
                    &user
                }
            ),
        )
        .into());
    }
    let cfg = config::load(config_dir, &machine.owner_home)?;
    Ok(Context { machine, cfg })
}

pub fn run(action: HypostasisAction, config_dir: &Path, sys: &dyn System) -> Result<u8> {
    let ctx = context(config_dir, sys)?;
    match action {
        HypostasisAction::Setup => setup::run(&ctx, sys),
        HypostasisAction::Start => start::run(&ctx, sys),
        HypostasisAction::Status => status::run(&ctx, sys),
        HypostasisAction::Stop => stop::run(&ctx, sys),
    }
}

/// Shared fixtures for unit tests in this module.
#[cfg(test)]
pub mod test_support {
    use std::path::Path;

    use super::*;

    pub fn context() -> Context {
        let file = Path::new("/cfg/hypostasis.yaml");
        let text = include_str!("../../tests/fixtures/hypostasis.yaml");
        let mapping = crate::config::parse_mapping(file, text).unwrap();
        let cfg = config::parse(
            Path::new("/cfg"),
            file,
            &mapping,
            Path::new("/home/kostadis"),
        )
        .unwrap();
        Context {
            machine: MachineConfig {
                allowed_network: "192.168.1.0/24".parse().unwrap(),
                owner: "kostadis".to_string(),
                owner_uid: 1000,
                owner_home: "/home/kostadis".into(),
            },
            cfg,
        }
    }

    pub fn authority() -> Authority {
        let hosts = vec!["localhost".to_string()];
        authority::parse(
            Path::new("/home/kostadis/.config/hypostasis/hypostasis.yaml"),
            include_str!("../../tests/fixtures/authority.yaml"),
            Path::new("/home/kostadis"),
            &hosts,
        )
        .unwrap()
    }
}
