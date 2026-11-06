//! Scoped mounts: a folder inside a site may be a symlink into an allowed root (ticket 0122),
//! so a pipeline work can hold the code and outputs its stages execute without them leaving
//! the web. With an allowed list set, the web follows a symlink inside a site for reads,
//! listings and writes only when it lands inside the web itself or inside an allowed root
//! (the projects directory, from config); a symlink that lands elsewhere is refused, not
//! followed. The check walks the path from the web's root and judges the symlinks *below a
//! site's top folder*: the web's root itself, and a site folder itself, stay whatever the
//! web chose to put there. Without an allowed list the web is lenient, as before mounts.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use walkdir::WalkDir;

use crate::error::{CoreError, Result};

/// The web's root, plus the roots a mount under a site may point into. Without an
/// allowed list the web is lenient, as it was before scoped mounts existed.
#[derive(Debug, Default)]
pub struct Mounts {
    root: PathBuf,
    allowed: RwLock<Option<Vec<PathBuf>>>,
}

impl Clone for Mounts {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            allowed: RwLock::new(self.allowed.read().expect("mounts lock").clone()),
        }
    }
}

impl Mounts {
    /// The web's root, no allowance to point into: lenient (kept for callers that had none).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed: RwLock::new(None),
        }
    }

    /// `allowed`: the roots a symlink under a site may resolve into besides the web itself
    /// (the projects directory, from config); a symlink landing elsewhere is refused.
    pub fn with_allowed(root: impl Into<PathBuf>, allowed: Vec<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed: RwLock::new(Some(allowed)),
        }
    }

    /// Swap the allowed list (config reload); pass `None` to go back to lenient.
    pub fn set_allowed(&self, allowed: Option<Vec<PathBuf>>) {
        *self.allowed.write().expect("mounts lock") = allowed;
    }

    /// Whether the web may read, list, write or watch `path` (its own address for it):
    /// every symlink below a site's top folder must land inside the web or an allowed root.
    pub fn check(&self, path: &Path) -> Result<()> {
        let guard = self.allowed.read().expect("mounts lock");
        let Some(allowed) = guard.as_ref() else {
            return Ok(());
        };
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return Ok(());
        };
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let mut resolved = root.clone();
        for (depth, part) in rel.iter().enumerate() {
            resolved.push(part);
            if depth == 0 || !is_symlink(&resolved) {
                continue;
            }
            let target = resolved.canonicalize().map_err(|e| CoreError::Io {
                path: resolved.clone(),
                source: e,
            })?;
            if !lands_inside(&target, &root, allowed) {
                return Err(CoreError::MountRefused {
                    path: path.to_path_buf(),
                });
            }
            resolved = target;
        }
        Ok(())
    }

    /// The same question as a bool, for walkers and watchers.
    pub fn allows(&self, path: &Path) -> bool {
        self.check(path).is_ok()
    }

    /// The mount points there are now, as web paths (a symlink at a site's top level, or
    /// deeper, that lands in the web or an allowed root). Only when an allowed list is set:
    /// without one there are no mounts, as before the feature.
    pub fn current(&self) -> Vec<PathBuf> {
        if self.allowed.read().expect("mounts lock").is_none() {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            for e in WalkDir::new(entry.path())
                .min_depth(1)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| e.file_type().is_dir() || e.file_type().is_symlink())
                .filter_map(|e| e.ok())
            {
                if e.path().is_dir() && self.allows(e.path()) {
                    found.push(e.path().to_path_buf());
                }
            }
        }
        found
    }
}

fn lands_inside(target: &Path, root: &Path, allowed: &[PathBuf]) -> bool {
    target.starts_with(root)
        || allowed
            .iter()
            .any(|a| target.starts_with(a.canonicalize().unwrap_or_else(|_| a.clone())))
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}
