//! The session's server terminals as this TUI shows them: a terminal model (`composer-term`)
//! per terminal it has open, fed from `terminal.output`; and the session's background
//! processes.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use composer_protocol::ProcessInfo;
use composer_term::{Sink, TermScreen};

/// Where a terminal's screen sends what it writes to its program: (terminal id, bytes).
pub type TerminalSink = Arc<dyn Fn(String, Vec<u8>) + Send + Sync>;

/// The terminals open on the stage, and the session's background processes.
#[derive(Default)]
pub struct TerminalStore {
    screens: HashMap<String, TermScreen>,
    /// Terminals whose scrollback was asked for and hasn't come.
    pub reading: HashSet<String>,
    /// A `terminal.open` is on its way: its answer brings the screen (no read meanwhile).
    pub opening: bool,
    /// Terminals that ended (or could not be read): shown with a note.
    pub ended: HashSet<String>,
    /// The session's background processes that are running.
    pub processes: Vec<ProcessInfo>,
    /// The size of the stage's terminal view (columns, rows), as last drawn.
    pub size: (u16, u16),
    sink: Option<TerminalSink>,
}

impl std::fmt::Debug for TerminalStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalStore")
            .field("screens", &self.screens.keys().collect::<Vec<_>>())
            .field("ended", &self.ended)
            .field("processes", &self.processes)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl TerminalStore {
    pub fn new() -> Self {
        Self {
            size: (80, 24),
            ..Self::default()
        }
    }

    /// Where every screen sends its program's bytes from now on (the app loop's dispatcher).
    pub fn set_sink(&mut self, sink: TerminalSink) {
        self.sink = Some(sink);
    }

    pub fn screen(&self, id: &str) -> Option<&TermScreen> {
        self.screens.get(id)
    }

    pub fn screen_mut(&mut self, id: &str) -> Option<&mut TermScreen> {
        self.screens.get_mut(id)
    }

    /// A screen for terminal `id`, its old output drawn: replacing one it had.
    pub fn open(&mut self, id: &str, scrollback: &[u8]) {
        let sink: Option<Sink> = self.sink.clone().map(|sink| {
            let id = id.to_string();
            Arc::new(move |bytes: Vec<u8>| sink(id.clone(), bytes)) as Sink
        });
        let (cols, rows) = self.size;
        let mut screen = TermScreen::with_sink(cols as usize, rows as usize, sink);
        screen.replay(scrollback);
        self.screens.insert(id.to_string(), screen);
        self.reading.remove(id);
        self.ended.remove(id);
        self.opening = false;
    }

    /// Output of terminal `id`; nothing when it has no screen here (its scrollback, asked for,
    /// will have it).
    pub fn output(&mut self, id: &str, bytes: &[u8]) {
        if let Some(screen) = self.screens.get_mut(id) {
            screen.feed(bytes);
        }
    }

    pub fn exited(&mut self, id: &str) {
        self.reading.remove(id);
        if self.ended.insert(id.to_string())
            && let Some(screen) = self.screens.get_mut(id)
        {
            screen.feed(b"\r\n\x1b[2m[the terminal ended]\x1b[0m\r\n");
        }
    }

    /// The background process running in terminal `id`, if it is one.
    pub fn process(&self, id: &str) -> Option<&ProcessInfo> {
        self.processes.iter().find(|p| p.terminal == id)
    }

    /// Only the terminals among `ids` stay (the desk's).
    pub fn keep(&mut self, ids: &[String]) {
        self.screens.retain(|id, _| ids.contains(id));
        self.reading.retain(|id| ids.contains(id));
    }

    /// Forget everything (another session).
    pub fn clear(&mut self) {
        self.screens.clear();
        self.reading.clear();
        self.ended.clear();
        self.processes.clear();
        self.opening = false;
    }
}
