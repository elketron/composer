use std::sync::Arc;

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::error::{Result, ServerError};
use crate::server::Server;

/// Watches the sites folder; every changed path is synced into the index and
/// broadcast. Keep the returned watcher alive for as long as watching should last.
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

    let server = Arc::downgrade(server);
    tokio::spawn(async move {
        while let Some(path) = rx.recv().await {
            let Some(server) = server.upgrade() else {
                return;
            };
            server.sync_path(&path);
        }
    });
    Ok(watcher)
}
