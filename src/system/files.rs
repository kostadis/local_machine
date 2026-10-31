//! Replace managed files so a failed action can put the previous version back.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::System;

/// The previous state of a replaced file, kept in memory until the action finishes.
pub struct Backup {
    path: PathBuf,
    previous: Option<String>,
}

impl Backup {
    pub fn restore(&self, sys: &dyn System) -> Result<()> {
        match &self.previous {
            Some(content) => sys.install_file(&self.path, content),
            None => sys.remove_file(&self.path),
        }
    }
}

pub fn replace(sys: &dyn System, path: &Path, content: &str) -> Result<Backup> {
    let previous = sys.read_file(path);
    sys.install_file(path, content)?;
    Ok(Backup {
        path: path.to_path_buf(),
        previous,
    })
}
