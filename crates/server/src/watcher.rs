use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

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
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        // Reads (inotify access/close-nowrite) are not changes; we read pages all the time.
        if let Ok(event) = event.as_ref()
            && !event.kind.is_access()
        {
            let created = event.kind.is_create();
            for path in &event.paths {
                let _ = tx.send((path.clone(), created));
            }
        }
    })
    .map_err(|e| ServerError::Invalid(format!("file watcher: {e}")))?;
    let root = server.workspace.root().to_path_buf();
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| ServerError::Invalid(format!("watching {}: {e}", root.display())))?;

    // Its folder, not the file: saving by rename replaces the file the watch would be on.
    let config_file = server.home.config_file();
    if let Some(home) = config_file.parent() {
        watcher
            .watch(home, RecursiveMode::NonRecursive)
            .map_err(|e| ServerError::Invalid(format!("watching {}: {e}", home.display())))?;
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
                Some(Some((path, _))) if path == config_file => {
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
