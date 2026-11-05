//! Posting watch hits to their sessions.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use composer_protocol::{ChatMessage, MessageSource, PageChangeKind, Role};

use crate::server::Server;
use crate::session::submit;
use crate::watches::WatchHit;

/// Hits are collected until none came for this long, then each session gets one message:
/// ten renders landing together are one turn, not ten.
const SETTLE: Duration = Duration::from_millis(500);

/// Hits that never pause (a progress file written every few hundred ms) are still delivered
/// this often: one session's busy watch must not hold back everyone's messages.
const MAX_DELAY: Duration = Duration::from_secs(2);

/// Delivers watch hits for as long as the server runs.
pub async fn run_watches(server: Arc<Server>) {
    let Some(mut hits) = server.watches.take_receiver() else {
        return;
    };
    let mut pending: BTreeMap<String, Vec<WatchHit>> = BTreeMap::new();
    let mut started = Instant::now();
    loop {
        let left = MAX_DELAY.saturating_sub(started.elapsed());
        let next = if pending.is_empty() {
            hits.recv().await.map(Some)
        } else if left.is_zero() {
            Some(None)
        } else {
            tokio::time::timeout(SETTLE.min(left), hits.recv())
                .await
                .map_or(Some(None), |next| next.map(Some))
        };
        match next {
            None => return,
            Some(Some(hit)) => {
                if pending.is_empty() {
                    started = Instant::now();
                }
                pending.entry(hit.session.clone()).or_default().push(hit);
            }
            Some(None) => {
                for (session, hits) in std::mem::take(&mut pending) {
                    deliver(&server, &session, &hits);
                }
            }
        }
    }
}

fn deliver(server: &Arc<Server>, session: &str, hits: &[WatchHit]) {
    let Ok(target) = server.sessions.get(session) else {
        tracing::info!("watches of session {session}: it's gone, dropping them");
        server.watches.drop_session(session);
        return;
    };
    let mut patterns: Vec<&str> = vec![];
    for hit in hits {
        if !patterns.contains(&hit.pattern.as_str()) {
            patterns.push(&hit.pattern);
        }
    }
    // Ended first, so the message says what is still watched.
    for pattern in &patterns {
        server.watches.delivered(session, pattern);
    }
    let message = ChatMessage {
        role: Role::User,
        content: message_text(server, session, hits),
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        attachments: vec![],
        source: Some(MessageSource::Watch {
            pattern: patterns.join(", "),
        }),
    };
    submit(server, &target, message);
    tracing::info!(
        "watch: {} change(s) posted to session {session}",
        hits.len()
    );
}

/// "Files changed: - created site://…" and what is still watched.
fn message_text(server: &Server, session: &str, hits: &[WatchHit]) -> String {
    let mut text = String::from("Files you watch changed:\n");
    // Each file once, as it was last changed (a watch on `**` and one on the file both hit).
    let mut files: Vec<(&str, PageChangeKind)> = vec![];
    for hit in hits {
        match files.iter_mut().find(|(url, _)| *url == hit.url) {
            // Created, then written to: still new.
            Some(file)
                if file.1 == PageChangeKind::Created && hit.kind == PageChangeKind::Modified => {}
            Some(file) => file.1 = hit.kind,
            None => files.push((&hit.url, hit.kind)),
        }
    }
    for (url, kind) in files {
        let kind = match kind {
            PageChangeKind::Created => "created",
            PageChangeKind::Modified => "modified",
            PageChangeKind::Deleted => "deleted",
        };
        text.push_str(&format!("- {kind} {url}\n"));
    }
    let still: Vec<String> = server
        .watches
        .for_session(session)
        .into_iter()
        .map(|w| w.pattern)
        .collect();
    if still.is_empty() {
        text.push_str("You watch nothing now; call `watch` again to keep watching.");
    } else {
        text.push_str(&format!("Still watching: {}.", still.join(", ")));
    }
    text
}
