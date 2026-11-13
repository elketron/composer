//! Scoped mounts: a folder inside a site may be a symlink into an allowed root (ticket 0122),
//! so a site's work can hold its code and outputs without them leaving
//! the web. With an allowed list set, the web follows a symlink inside a site for reads,
//! listings and writes only when it lands inside the web itself or inside an allowed root
//! (the projects directory, from config); a symlink that lands elsewhere is refused, not
//! followed. The check walks the path from the web's root and judges the symlinks *below a
//! site's top folder*: the web's root itself, and a site folder itself, stay whatever the
//! web chose to put there, except that the guard (another workspace's folders) refuses a site
//! folder too. Without an allowed list the web is lenient, as before mounts.
//!
//! Project mounts are the same mechanism with a mode: a work's `project/` is registered as
//! `link → (target, mode)`, so reads resolve through it and `check_write` can refuse a `read`
//! work even when the symlink itself would be allowed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use walkdir::WalkDir;

use crate::error::{CoreError, Result};

/// Says whether a mount's (canonical) target must be refused even inside an allowed root: the
/// server uses it for folders another workspace owns.
pub type MountGuard = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

/// What a registered project mount allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MountMode {
    /// Reads only.
    #[default]
    Read,
    /// The project in place.
    Write,
    /// The ticket's worktree.
    Worktree,
}

impl MountMode {
    pub fn readonly(self) -> bool {
        matches!(self, Self::Read)
    }
}

/// The web's root, plus the roots a mount under a site may point into. Without an
/// allowed list the web is lenient, as it was before scoped mounts existed.
#[derive(Default)]
pub struct Mounts {
    root: PathBuf,
    allowed: RwLock<Option<Vec<PathBuf>>>,
    refuse: RwLock<Option<MountGuard>>,
    /// `link → (target, mode)` for work `project/` mounts.
    projects: RwLock<BTreeMap<PathBuf, (PathBuf, MountMode)>>,
}

impl Clone for Mounts {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            allowed: RwLock::new(self.allowed.read().expect("mounts lock").clone()),
            refuse: RwLock::new(self.refuse.read().expect("mounts lock").clone()),
            projects: RwLock::new(self.projects.read().expect("projects lock").clone()),
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
            .field(
                "projects",
                &self.projects.read().expect("projects lock").len(),
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
            projects: RwLock::new(BTreeMap::new()),
        }
    }

    /// `allowed`: the roots a symlink under a site may resolve into besides the web itself
    /// (the projects directory, from config); a symlink landing elsewhere is refused.
    pub fn with_allowed(root: impl Into<PathBuf>, allowed: Vec<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed: RwLock::new(Some(allowed)),
            refuse: RwLock::new(None),
            projects: RwLock::new(BTreeMap::new()),
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

    /// Register a work's `project/` link: the target is the project root or the ticket worktree.
    pub fn set_project(&self, link: PathBuf, target: PathBuf, mode: MountMode) {
        self.projects
            .write()
            .expect("projects lock")
            .insert(link, (target, mode));
    }

    /// Whether the web may read `path` (its own address for it): every symlink below a site's
    /// top folder must land inside the web or an allowed root. A registered project mount is
    /// allowed by construction, but still checked against the guard.
    pub fn check(&self, path: &Path) -> Result<()> {
        if let Some((link, target, _)) = self.project_for(path) {
            let target = target.join(path.strip_prefix(&link).unwrap_or(Path::new("")));
            let refused = self
                .refuse
                .read()
                .expect("mounts lock")
                .as_ref()
                .is_some_and(|refuse| refuse(&target));
            if refused {
                return Err(CoreError::MountRefused {
                    path: path.to_path_buf(),
                });
            }
            return self.check_allowed(&target);
        }
        self.check_allowed(path)
    }

    /// The same, for a write. A `read` project mount is refused; a `write`/`worktree` one is
    /// checked like any other path.
    pub fn check_write(&self, path: &Path) -> Result<()> {
        if let Some((_, _, mode)) = self.project_for(path)
            && mode.readonly()
        {
            return Err(CoreError::MountRefused {
                path: path.to_path_buf(),
            });
        }
        self.check(path)
    }

    fn check_allowed(&self, path: &Path) -> Result<()> {
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

    /// Whether `path` is under a registered project mount.
    pub fn is_project(&self, path: &Path) -> bool {
        self.project_for(path).is_some()
    }

    /// The live mode of the project mount `path` is under.
    pub fn project_mode(&self, path: &Path) -> Option<MountMode> {
        self.project_for(path).map(|(_, _, mode)| mode)
    }

    /// The project mount `path` is under, if any.
    fn project_for(&self, path: &Path) -> Option<(PathBuf, PathBuf, MountMode)> {
        self.projects
            .read()
            .expect("projects lock")
            .iter()
            .find(|(link, _)| path.starts_with(link))
            .map(|(link, (target, mode))| (link.clone(), target.clone(), *mode))
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
    /// deeper, that lands in the web or an allowed root), plus every registered project
    /// link. Only symlinks count: a real folder is already on the watched tree, and listing
    /// it as a mount would make the watcher watch the web to itself twice. Only when an
    /// allowed list is set: without one there are no mounts, as before the feature.
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
        found.extend(self.projects.read().expect("projects lock").keys().cloned());
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
