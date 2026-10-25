use std::sync::Arc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::error::{Result, ServerError};
use crate::server::Server;

/// Editors write a file in several steps (truncate, write, rename): reload once they're done.
const CONFIG_SETTLE: Duration = Duration::from_millis(300);

/// Watches the sites folder; every changed path is synced into the index and
/// broadcast. Also `config.toml`: a change reloads it. Keep the returned watcher alive for as
/// long as watching should last.
pub fn watch(server: &Arc<Server>) -> Result<notify::RecommendedWatcher> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        // Reads (inotify access/close-nowrite) are not changes; we read pages all the time.
        if let Ok(event) = event.as_ref()
            && !event.kind.is_access()
        {
            for path in &event.paths {
                let _ = tx.send(path.clone());
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
        while let Some(path) = rx.recv().await {
            let Some(server) = server.upgrade() else {
                return;
            };
            if path == config_file {
                let _ = config_tx.send(());
            } else {
                server.sync_path(&path);
            }
        }
    });
    Ok(watcher)
}
