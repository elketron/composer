//! Scoped mounts: a folder inside a site may be a symlink into an allowed root (ticket 0122),
//! so a pipeline work can hold the code and outputs its stages execute without them leaving
//! the web. With an allowed list set, the web follows a symlink inside a site for reads,
//! listings and writes only when it lands inside the web itself or inside an allowed root
//! (the projects directory, from config); a symlink that lands elsewhere is refused, not
//! followed. The check walks the path from the web's root and judges the symlinks *below a
//! site's top folder*: the web's root itself, and a site folder itself, stay whatever the
//! web chose to put there, except that the guard (another workspace's folders) refuses a site
//! folder too. Without an allowed list the web is lenient, as before mounts.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use walkdir::WalkDir;

use crate::error::{CoreError, Result};

/// Says whether a mount's (canonical) target must be refused even inside an allowed root: the
/// server uses it for folders another workspace owns.
pub type MountGuard = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

/// The web's root, plus the roots a mount under a site may point into. Without an
/// allowed list the web is lenient, as it was before scoped mounts existed.
#[derive(Default)]
pub struct Mounts {
    root: PathBuf,
    allowed: RwLock<Option<Vec<PathBuf>>>,
    refuse: RwLock<Option<MountGuard>>,
}

impl Clone for Mounts {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            allowed: RwLock::new(self.allowed.read().expect("mounts lock").clone()),
            refuse: RwLock::new(self.refuse.read().expect("mounts lock").clone()),
        }
    }
}

impl std::fmt::Debug for Mounts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mounts")
            .field("root", &self.root)
            .field("allowed", &self.allowed.read().expect("mounts lock"))
            .field(
                "guarded",
                &self.refuse.read().expect("mounts lock").is_some(),
            )
            .finish()
    }
}

impl Mounts {
    /// The web's root, no allowance to point into: lenient (kept for callers that had none).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed: RwLock::new(None),
            refuse: RwLock::new(None),
        }
    }

    /// `allowed`: the roots a symlink under a site may resolve into besides the web itself
    /// (the projects directory, from config); a symlink landing elsewhere is refused.
    pub fn with_allowed(root: impl Into<PathBuf>, allowed: Vec<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed: RwLock::new(Some(allowed)),
            refuse: RwLock::new(None),
        }
    }

    /// Swap the allowed list (config reload); pass `None` to go back to lenient.
    pub fn set_allowed(&self, allowed: Option<Vec<PathBuf>>) {
        *self.allowed.write().expect("mounts lock") = allowed;
    }

    /// Refuse mounts whose target `refuse` says no to, even inside an allowed root.
    pub fn set_guard(&self, refuse: MountGuard) {
        *self.refuse.write().expect("mounts lock") = Some(refuse);
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
            if !is_symlink(&resolved) {
                continue;
            }
            let target = resolved.canonicalize().map_err(|e| CoreError::Io {
                path: resolved.clone(),
                source: e,
            })?;
            let refused = self
                .refuse
                .read()
                .expect("mounts lock")
                .as_ref()
                .is_some_and(|refuse| refuse(&target));
            // A site folder that is a symlink may land anywhere (the web chose to put it
            // there), but never where the guard refuses.
            let outside = depth > 0 && !lands_inside(&target, &root, allowed);
            if refused || outside {
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

    /// Whether walks should follow symlinks here: only once an allowed list is set —
    /// without one the web behaves as it did before mounts (listings do not descend).
    pub fn follows_links(&self) -> bool {
        self.allowed.read().expect("mounts lock").is_some()
    }

    /// The mount points there are now, as web paths (a symlink at a site's top level, or
    /// deeper, that lands in the web or an allowed root). Only symlinks count: a real
    /// folder is already on the watched tree, and listing it as a mount would make the
    /// watcher watch the web to itself twice. Only when an allowed list is set: without
    /// one there are no mounts, as before the feature.
    pub fn current(&self) -> Vec<PathBuf> {
        if self.allowed.read().expect("mounts lock").is_none() {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if is_symlink(&path) {
                // A site folder that is itself a symlink into an allowed root is a mount
                // too: judged lenient as ever (the web chose to put it there), listed here
                // so the mount watcher covers it.
                found.push(path);
                continue;
            }
            for e in WalkDir::new(path)
                .min_depth(1)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| e.file_type().is_dir() || e.file_type().is_symlink())
                .filter_map(|e| e.ok())
            {
                if e.path_is_symlink() && self.allows(e.path()) {
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
