//! Warn about symlinks that lead into excluded names (research R7; warn-only by owner decision).

use std::path::{Path, PathBuf};

pub const MAX_DEPTH: usize = 4;

#[derive(Debug, Default)]
pub struct Scan {
    /// (link, resolved target)
    pub warnings: Vec<(PathBuf, PathBuf)>,
    pub depth_limit_hit: bool,
}

/// Walk `root` up to [`MAX_DEPTH`] levels, skipping excluded directories and never
/// following symlinked directories.
pub fn scan(root: &Path, exclude: &[String]) -> Scan {
    let mut result = Scan::default();
    walk(root, exclude, 1, &mut result);
    result
}

fn walk(dir: &Path, exclude: &[String], depth: usize, result: &mut Scan) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if meta.file_type().is_symlink() {
            if let Ok(target) = std::fs::canonicalize(&path)
                && target
                    .components()
                    .any(|c| exclude.iter().any(|e| c.as_os_str() == e.as_str()))
            {
                result.warnings.push((path, target));
            }
        } else if meta.is_dir() && !exclude.contains(&name) {
            if depth >= MAX_DEPTH {
                result.depth_limit_hit = true;
            } else {
                walk(&path, exclude, depth + 1, result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn finds_links_into_excluded_names() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path();
        std::fs::create_dir(root.join(".ssh")).unwrap();
        std::fs::create_dir_all(root.join("notes/a")).unwrap();
        symlink(root.join(".ssh"), root.join("keys-link")).unwrap();
        symlink(root.join("notes"), root.join("notes-link")).unwrap();
        symlink(root.join(".ssh"), root.join("notes/a/deep-link")).unwrap();

        let exclude = vec![".ssh".to_string(), ".gnupg".to_string()];
        let scan = scan(root, &exclude);
        let links: Vec<_> = scan
            .warnings
            .iter()
            .map(|(l, _)| l.strip_prefix(root).unwrap().to_path_buf())
            .collect();
        assert!(links.contains(&PathBuf::from("keys-link")));
        assert!(links.contains(&PathBuf::from("notes/a/deep-link")));
        assert!(!links.contains(&PathBuf::from("notes-link")));
        assert!(!scan.depth_limit_hit);
    }

    #[test]
    fn reports_depth_limit() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("1/2/3/4/5")).unwrap();
        assert!(scan(home.path(), &[]).depth_limit_hit);
    }
}
