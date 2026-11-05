use std::path::PathBuf;
use std::sync::Mutex;

use composer_protocol::PageChangeKind;
use tokio::sync::mpsc;

use crate::api::glob::url_matches;
use crate::watches::{FileWatch, WatchHit};

/// Every session's watches, kept in `state/watches.json` so they outlive a restart. Changes
/// are matched as they happen ([`WatchRegistry::changed`]); the hits queue up for
/// [`crate::watches::run_watches`], which posts them.
pub struct WatchRegistry {
    /// `None` in a worker process: its watches (if any) live with the parent.
    file: Option<PathBuf>,
    watches: Mutex<Vec<FileWatch>>,
    hits: mpsc::UnboundedSender<WatchHit>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<WatchHit>>>,
}

impl WatchRegistry {
    /// The registry kept in `file`; a missing or unreadable file starts it empty.
    pub fn load(file: PathBuf) -> Self {
        let watches = match std::fs::read_to_string(&file) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("{}: {e}; starting without watches", file.display());
                vec![]
            }),
            Err(_) => vec![],
        };
        Self::with(Some(file), watches)
    }

    /// Kept in memory only.
    pub fn in_memory() -> Self {
        Self::with(None, vec![])
    }

    fn with(file: Option<PathBuf>, watches: Vec<FileWatch>) -> Self {
        let (hits, receiver) = mpsc::unbounded_channel();
        Self {
            file,
            watches: Mutex::new(watches),
            hits,
            receiver: Mutex::new(Some(receiver)),
        }
    }

    /// Adds (or updates) a watch. Returns false if the session already had it, as given.
    pub fn add(&self, watch: FileWatch) -> bool {
        let mut watches = self.watches.lock().expect("watches lock");
        if watches.contains(&watch) {
            return false;
        }
        watches.retain(|w| !(w.session == watch.session && w.pattern == watch.pattern));
        watches.push(watch);
        self.save(&watches);
        true
    }

    /// Removes a session's watch. Returns false if it had none on `pattern`.
    pub fn remove(&self, session: &str, pattern: &str) -> bool {
        let mut watches = self.watches.lock().expect("watches lock");
        let before = watches.len();
        watches.retain(|w| !(w.session == session && w.pattern == pattern));
        let removed = watches.len() != before;
        if removed {
            self.save(&watches);
        }
        removed
    }

    /// Drops every watch of a session (it's gone).
    pub fn drop_session(&self, session: &str) {
        let mut watches = self.watches.lock().expect("watches lock");
        let before = watches.len();
        watches.retain(|w| w.session != session);
        if watches.len() != before {
            self.save(&watches);
        }
    }

    pub fn for_session(&self, session: &str) -> Vec<FileWatch> {
        self.watches
            .lock()
            .expect("watches lock")
            .iter()
            .filter(|w| w.session == session)
            .cloned()
            .collect()
    }

    /// A file changed: queues a hit for every watch it matches, except the writer's own
    /// (`writer`: the session whose tool call made the change).
    pub fn changed(&self, url: &str, kind: PageChangeKind, writer: Option<&str>) {
        let watches = self.watches.lock().expect("watches lock");
        for w in watches.iter() {
            if Some(w.session.as_str()) != writer && url_matches(&w.pattern, url) {
                let _ = self.hits.send(WatchHit {
                    session: w.session.clone(),
                    pattern: w.pattern.clone(),
                    url: url.to_string(),
                    kind,
                });
            }
        }
    }

    /// A message for `pattern` went out: a `once` watch on it ends (the whole burst it
    /// collected went with that message).
    pub fn delivered(&self, session: &str, pattern: &str) {
        let mut watches = self.watches.lock().expect("watches lock");
        let before = watches.len();
        watches.retain(|w| !(w.session == session && w.pattern == pattern && w.once));
        if watches.len() != before {
            self.save(&watches);
        }
    }

    /// The queued hits, for the one task that delivers them.
    pub fn take_receiver(&self) -> Option<mpsc::UnboundedReceiver<WatchHit>> {
        self.receiver.lock().expect("receiver lock").take()
    }

    fn save(&self, watches: &[FileWatch]) {
        let Some(file) = &self.file else {
            return;
        };
        let text = serde_json::to_string_pretty(watches).expect("watches serialize");
        if let Err(e) = std::fs::write(file, text) {
            tracing::warn!("{}: {e}", file.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(session: &str, pattern: &str, once: bool) -> FileWatch {
        FileWatch {
            session: session.into(),
            pattern: pattern.into(),
            once,
        }
    }

    #[test]
    fn matches_skips_the_writer_and_ends_once_watches_when_delivered() {
        let registry = WatchRegistry::in_memory();
        let mut hits = registry.take_receiver().unwrap();
        assert!(registry.add(watch("a", "site://video/renders/**", true)));
        assert!(registry.add(watch("b", "site://video/renders/*", false)));
        assert!(!registry.add(watch("b", "site://video/renders/*", false)));

        registry.changed("site://video/notes.md", PageChangeKind::Modified, None);
        assert!(hits.try_recv().is_err());

        // Its own write: not reported to `a`, which keeps watching.
        registry.changed(
            "site://video/renders/a.mp4",
            PageChangeKind::Created,
            Some("a"),
        );
        assert_eq!(hits.try_recv().unwrap().session, "b");
        assert!(hits.try_recv().is_err());

        registry.changed("site://video/renders/b.mp4", PageChangeKind::Created, None);
        let got: Vec<_> = [hits.try_recv().unwrap(), hits.try_recv().unwrap()]
            .into_iter()
            .map(|h| h.session)
            .collect();
        assert_eq!(got, ["a", "b"]);
        registry.delivered("a", "site://video/renders/**");
        registry.delivered("b", "site://video/renders/*");
        assert!(registry.for_session("a").is_empty());
        assert_eq!(registry.for_session("b").len(), 1);
    }

    #[test]
    fn kept_in_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("watches.json");
        let registry = WatchRegistry::load(file.clone());
        registry.add(watch("a", "site://x/**", false));
        assert_eq!(
            WatchRegistry::load(file.clone()).for_session("a"),
            [watch("a", "site://x/**", false)]
        );
        assert!(registry.remove("a", "site://x/**"));
        assert!(WatchRegistry::load(file).for_session("a").is_empty());
    }
}
