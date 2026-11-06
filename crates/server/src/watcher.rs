use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::server::is_watched;

use crate::error::{Result, ServerError};
use crate::server::Server;

/// Editors write a file in several steps (truncate, write, rename): reload once they're done.
const CONFIG_SETTLE: Duration = Duration::from_millis(300);

/// Changes in the sites are collected until none came for this long, then synced once per path:
/// a burst of writes is one event, and an editor's temp file (written, then renamed over the
/// page) has come and gone by then, so it makes none.
const SITES_SETTLE: Duration = Duration::from_millis(100);

/// A burst that never pauses (a log being appended to) is still synced this often.
const SITES_MAX_DELAY: Duration = Duration::from_secs(1);

/// Watches the sites folder; every changed path is synced into the index and
/// broadcast (`page.changed`, any file, not only pages), after a short settle ([`SITES_SETTLE`]). Also `config.toml`: a change reloads it. Keep the returned watcher alive for as
/// long as watching should last.
pub fn watch(server: &Arc<Server>) -> Result<notify::RecommendedWatcher> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let make = |tx: mpsc::UnboundedSender<(PathBuf, bool)>, follow_symlinks: bool| {
        let config = notify::Config::default().with_follow_symlinks(follow_symlinks);
        notify::RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                match event.as_ref() {
                    // Reads (inotify access/close-nowrite) are not changes; we read pages all the time.
                    Ok(event) if !event.kind.is_access() => {
                        let created = event.kind.is_create();
                        for path in &event.paths {
                            let _ = tx.send((path.clone(), created));
                        }
                    }
                    Ok(_) => {}
                    // Errors reach here when the backend cannot watch something, e.g. the
                    // inotify watch budget ran out (MaxFilesWatch): report, do not drop.
                    Err(e) => tracing::warn!("file watch: {e}"),
                }
            },
            config,
        )
    };
    // The root watcher follows no symlinks: a refused mount is not walked or watched at all,
    // and every mount is watched through the mount watcher below (its own walk does follow,
    // starting at the mount point, so events still carry the web's address for the file).
    let mut watcher =
        make(tx.clone(), false).map_err(|e| ServerError::Invalid(format!("file watcher: {e}")))?;
    let root = server.web.root().to_path_buf();
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| ServerError::Invalid(format!("watching {}: {e}", root.display())))?;

    // Their folders, not the files: saving by rename replaces the file the watch would be on.
    // The workspace's config and the global one it sits on top of both reload the config.
    let config_files = [server.home.config_file(), server.home.global_config_file()];
    let mut folders: Vec<&std::path::Path> =
        config_files.iter().filter_map(|f| f.parent()).collect();
    folders.dedup();
    for folder in folders.into_iter().filter(|f| f.is_dir()) {
        watcher
            .watch(folder, RecursiveMode::NonRecursive)
            .map_err(|e| ServerError::Invalid(format!("watching {}: {e}", folder.display())))?;
    }

    let (config_tx, mut config_rx) = mpsc::unbounded_channel::<()>();
    let weak = Arc::downgrade(server);
    tokio::spawn(async move {
        while config_rx.recv().await.is_some() {
            tokio::time::sleep(CONFIG_SETTLE).await;
            while config_rx.try_recv().is_ok() {}
            let Some(server) = weak.upgrade() else {
                return;
            };
            server.config_file_changed();
        }
    });

    let server = Arc::downgrade(server);
    tokio::spawn(async move {
        // Mounts get their own watcher (mounted), with follow on so its walk descends into
        // the mount itself: the root watcher leaves symlinks alone, and the inotify backend
        // never attaches to a symlinked folder created while running (it reports the link as
        // a file, so the dynamic attach only fires for ISDIR). Watching the mount point keeps
        // the paths as they hang off the web root, which is how the web names them.
        let mut mounted = match make(tx.clone(), true) {
            Ok(watcher) => Some(watcher),
            Err(e) => {
                tracing::warn!("mount watcher: {e}; mounted folders will not sync");
                None
            }
        };
        let Some(server) = server.upgrade() else {
            return;
        };
        let root = server.web.root().to_path_buf();
        for path in server.web.mount_points() {
            if is_watched(&root, &path) {
                watch_mount(&mut mounted, &path);
            }
        }
        let server = Arc::downgrade(&server);
        // Each path once, and whether it was created within the burst.
        let mut burst: HashMap<PathBuf, bool> = HashMap::new();
        let mut started = Instant::now();
        loop {
            let left = SITES_MAX_DELAY.saturating_sub(started.elapsed());
            let next = if burst.is_empty() {
                rx.recv().await.map(Some)
            } else if left.is_zero() {
                Some(None)
            } else {
                tokio::time::timeout(SITES_SETTLE.min(left), rx.recv())
                    .await
                    .map_or(Some(None), |next| next.map(Some))
            };
            match next {
                None => return,
                Some(Some((path, _))) if config_files.contains(&path) => {
                    let _ = config_tx.send(());
                }
                Some(Some((path, created))) => {
                    if burst.is_empty() {
                        started = Instant::now();
                    }
                    burst.entry(path).or_insert(created);
                }
                // Settled (or waited long enough): sync what changed.
                Some(None) => {
                    let Some(server) = server.upgrade() else {
                        return;
                    };
                    for (path, created) in burst.drain() {
                        // A mount made while running: attach to it, or changes made on the
                        // project side would never light up (ticket 0122).
                        if created && server.mount_created(&path) {
                            watch_mount(&mut mounted, &path);
                        }
                        // Born and gone within the burst (a temp file): nothing happened.
                        if !created || path.exists() {
                            server.sync_path(&path);
                        }
                    }
                }
            }
        }
    });
    Ok(watcher)
}

/// Attach the mount watcher to a mount folder (recursive, so its whole tree is reported).
fn watch_mount(mounted: &mut Option<notify::RecommendedWatcher>, path: &Path) {
    let Some(watcher) = mounted else {
        return;
    };
    if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
        tracing::warn!("watching mount {}: {e}", path.display());
    }
}
