use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use composer_protocol::{Seq, ServerMessage};

use crate::error::{Result, ServerError};
use crate::hub::EventHub;
use crate::session::TranscriptInner;

/// A session's event log (`events.jsonl`): what clients replay on attach.
///
/// Assigning a seq, persisting and broadcasting happen under one lock, so a
/// client attaching concurrently sees every event exactly once: either in the
/// replay or live. Streaming deltas get a seq but are not persisted.
#[derive(Debug)]
pub struct Transcript {
    path: PathBuf,
    inner: Mutex<TranscriptInner>,
}

impl Transcript {
    /// Opens (or creates) the log. Seqs continue after `min_last` and after
    /// anything already in the file.
    pub fn open(path: &Path, min_last: Seq) -> Result<Self> {
        let last_in_file = read_events(path)?
            .iter()
            .filter_map(seq_of)
            .max()
            .unwrap_or(0);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| ServerError::io(path, e))?;
        Ok(Self {
            path: path.to_path_buf(),
            inner: Mutex::new(TranscriptInner {
                next_seq: last_in_file.max(min_last) + 1,
                file,
            }),
        })
    }

    /// Builds an event with the next seq, persists it (unless ephemeral) and broadcasts it.
    pub fn emit(&self, hub: &EventHub, make: impl FnOnce(Seq) -> ServerMessage) -> Seq {
        let mut inner = self.inner.lock().expect("transcript lock");
        let seq = inner.next_seq;
        inner.next_seq += 1;
        let message = make(seq);
        if !is_ephemeral(&message) {
            let line = serde_json::to_string(&message).expect("events serialize");
            if let Err(e) = writeln!(inner.file, "{line}") {
                tracing::error!("writing {}: {e}", self.path.display());
            }
        }
        hub.send(message);
        seq
    }

    /// Calls `f` with the persisted events after `since` and the last assigned
    /// seq, while holding the lock (no events are emitted meanwhile).
    pub fn replay<T>(
        &self,
        since: Option<Seq>,
        f: impl FnOnce(Vec<ServerMessage>, Seq) -> T,
    ) -> Result<T> {
        let inner = self.inner.lock().expect("transcript lock");
        let events = read_events(&self.path)?
            .into_iter()
            .filter(|e| since.is_none_or(|s| seq_of(e).is_some_and(|seq| seq > s)))
            .collect();
        Ok(f(events, inner.next_seq - 1))
    }

    pub fn last_seq(&self) -> Seq {
        self.inner.lock().expect("transcript lock").next_seq - 1
    }
}

/// Streaming deltas: sent live, superseded by `message.done`, not persisted.
pub fn is_ephemeral(message: &ServerMessage) -> bool {
    matches!(
        message,
        ServerMessage::ThinkingDelta { .. } | ServerMessage::MessageDelta { .. }
    )
}

fn read_events(path: &Path) -> Result<Vec<ServerMessage>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ServerError::io(path, e)),
    };
    Ok(BufReader::new(file)
        .lines()
        .map_while(|line| line.ok())
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect())
}

fn seq_of(message: &ServerMessage) -> Option<Seq> {
    match message {
        ServerMessage::TurnStarted { seq, .. }
        | ServerMessage::TurnQueued { seq, .. }
        | ServerMessage::TurnDone { seq, .. }
        | ServerMessage::ThinkingDelta { seq, .. }
        | ServerMessage::MessageDelta { seq, .. }
        | ServerMessage::MessageDone { seq, .. }
        | ServerMessage::ToolCallStarted { seq, .. }
        | ServerMessage::ToolResult { seq, .. }
        | ServerMessage::AgentNavigated { seq, .. } => Some(*seq),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_replays_and_continues_seq() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let hub = EventHub::new(16);
        let t = Transcript::open(&path, 0).unwrap();
        t.emit(&hub, |seq| ServerMessage::TurnStarted {
            session: "s".into(),
            seq,
        });
        t.emit(&hub, |seq| ServerMessage::MessageDelta {
            session: "s".into(),
            seq,
            text: "x".into(),
        });
        t.emit(&hub, |seq| ServerMessage::TurnDone {
            session: "s".into(),
            seq,
            cancelled: false,
        });

        let (events, last) = t.replay(None, |e, last| (e, last)).unwrap();
        assert_eq!(events.len(), 2, "delta not persisted");
        assert_eq!(last, 3);
        let after_1 = t.replay(Some(1), |e, _| e).unwrap();
        assert!(matches!(
            after_1[..],
            [ServerMessage::TurnDone { seq: 3, .. }]
        ));

        drop(t);
        let reopened = Transcript::open(&path, 5).unwrap();
        assert_eq!(
            reopened.emit(&hub, |seq| ServerMessage::TurnStarted {
                session: "s".into(),
                seq
            }),
            6
        );
    }
}
