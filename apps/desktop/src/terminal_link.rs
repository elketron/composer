use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use composer_client::Connection;
use composer_protocol::{ClientCommand, ClientRequest};

/// A server terminal on this window's connection: what a pane types and its size go here. Replies
/// (`done`) come back as events and are ignored; nothing is sent while disconnected.
#[derive(Clone)]
pub struct TerminalLink {
    connection: Connection,
    terminal: String,
    ids: Arc<AtomicU64>,
    /// While set, nothing is sent: the pane is replaying old output, whose queries were
    /// answered when they were new (answering again would type into the program).
    muted: Arc<AtomicBool>,
}

impl TerminalLink {
    pub fn new(connection: Connection, terminal: String, ids: Arc<AtomicU64>) -> Self {
        Self {
            connection,
            terminal,
            ids,
            muted: Arc::default(),
        }
    }

    /// Runs `replay` with sending muted.
    pub fn muted<T>(&self, replay: impl FnOnce() -> T) -> T {
        self.muted.store(true, Ordering::SeqCst);
        let out = replay();
        self.muted.store(false, Ordering::SeqCst);
        out
    }

    pub fn terminal(&self) -> &str {
        &self.terminal
    }

    pub fn input(&self, bytes: &[u8]) {
        self.send(ClientCommand::TerminalInput {
            terminal: self.terminal.clone(),
            data: String::from_utf8_lossy(bytes).into_owned(),
        });
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        self.send(ClientCommand::TerminalResize {
            terminal: self.terminal.clone(),
            cols,
            rows,
        });
    }

    fn send(&self, command: ClientCommand) {
        if self.muted.load(Ordering::SeqCst) {
            return;
        }
        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        let _ = self.connection.send(ClientRequest { id, command });
    }
}

/// The terminal model's writer: everything it encodes (keys, paste, answers to queries).
pub struct LinkWriter(pub TerminalLink);

impl std::io::Write for LinkWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.input(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
