use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use composer_protocol::ServerMessage;
use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::apps::ProjectName;
use crate::apps::project_facts::SKIPPED;
use crate::server::Server;

/// Changes are collected until none came for this long, then sent as one `project.changed`.
const SETTLE: Duration = Duration::from_millis(200);
/// Folders watched at most: a project's own folders, not its dependencies or build output.
const MAX_FOLDERS: usize = 4_000;
/// Paths per event at most.
const MAX_PATHS: usize = 200;

/// The watch on an open project's files: each folder on its own (not recursively, so
/// `node_modules`, `target`, `.git` and the like are never watched), new folders added as they
/// appear. Dropped: the watch stops.
pub struct ProjectWatch {
    _watcher: Arc<Mutex<notify::RecommendedWatcher>>,
}

impl ProjectWatch {
    /// Watches `folder` (project `name`) and broadcasts `project.changed` from `server`.
    pub fn start(server: &Arc<Server>, name: &ProjectName, folder: &Path) -> Result<Self, String> {
        let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();
        let watcher = notify::RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| match event {
                Ok(event) if !event.kind.is_access() => {
                    for path in event.paths {
                        let _ = tx.send(path);
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("project watch: {e}"),
            },
            notify::Config::default(),
        )
        .map_err(|e| format!("watching {}: {e}", folder.display()))?;
        let watcher = Arc::new(Mutex::new(watcher));
        let mut watched = 0;
        for dir in folders(folder) {
            if watched >= MAX_FOLDERS {
                tracing::warn!("project {name}: watching its first {MAX_FOLDERS} folders only");
                break;
            }
            let added = watcher
                .lock()
                .expect("project watch lock")
                .watch(&dir, RecursiveMode::NonRecursive);
            if added.is_ok() {
                watched += 1;
            }
        }
        let (server, name, folder) = (
            Arc::downgrade(server),
            name.to_string(),
            folder.to_path_buf(),
        );
        let adding = Arc::downgrade(&watcher);
        tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                let mut paths = BTreeSet::from([first]);
                while let Ok(Some(more)) = tokio::time::timeout(SETTLE, rx.recv()).await {
                    paths.insert(more);
                }
                if !report(&server, &adding, &name, &folder, paths) {
                    return;
                }
            }
        });
        Ok(Self { _watcher: watcher })
    }
}

/// Broadcasts a settled burst (and watches the folders it created); `false` once the server or
/// the watch is gone.
fn report(
    server: &Weak<Server>,
    watcher: &Weak<Mutex<notify::RecommendedWatcher>>,
    name: &str,
    folder: &Path,
    paths: BTreeSet<PathBuf>,
) -> bool {
    let (Some(server), Some(watcher)) = (server.upgrade(), watcher.upgrade()) else {
        return false;
    };
    let mut paths = paths;
    // A new folder is watched from now on; what was written into it before that is in it
    // already: walk it.
    let new_folders: Vec<PathBuf> = paths
        .iter()
        .filter(|p| p.is_dir() && p.strip_prefix(folder).is_ok_and(|r| !skipped(r)))
        .cloned()
        .collect();
    for new in new_folders {
        for dir in folders(&new) {
            let _ = watcher
                .lock()
                .expect("project watch lock")
                .watch(&dir, RecursiveMode::NonRecursive);
        }
        paths.extend(files_in(&new));
    }
    let relative: Vec<String> = paths
        .iter()
        .filter_map(|p| p.strip_prefix(folder).ok())
        .filter(|p| !skipped(p))
        .map(|p| p.display().to_string())
        .filter(|p| !p.is_empty())
        .take(MAX_PATHS)
        .collect();
    if !relative.is_empty() {
        server.hub.send(ServerMessage::ProjectChanged {
            project: name.to_string(),
            paths: relative,
        });
    }
    true
}

/// A path under a folder no watch enters.
fn skipped(relative: &Path) -> bool {
    relative
        .components()
        .any(|c| SKIPPED.iter().any(|s| c.as_os_str() == *s))
}

/// The files under `folder`, minus the skipped folders (at most a burst's worth).
fn files_in(folder: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !SKIPPED.iter().any(|s| e.file_name() == *s))
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .take(MAX_PATHS)
        .collect()
}

/// `folder` and its folders, minus the skipped ones (no symlinks followed).
fn folders(folder: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !SKIPPED.iter().any(|s| e.file_name() == *s))
        .flatten()
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.path().to_path_buf())
        .take(MAX_FOLDERS + 1)
        .collect()
}
