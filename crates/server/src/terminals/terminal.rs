use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use portable_pty::{Child, MasterPty, PtySize};

use crate::terminals::Scrollback;

/// One server-owned terminal of a session (ticket 0176): the user's shell on a
/// PTY and its recent output.
pub struct Terminal {
    pub id: String,
    /// The session it belongs to.
    pub session: String,
    pub cwd: PathBuf,
    pub(crate) master: Mutex<Box<dyn MasterPty + Send>>,
    pub(crate) writer: Mutex<Box<dyn Write + Send>>,
    pub(crate) child: Mutex<Box<dyn Child + Send + Sync>>,
    pub(crate) scrollback: Mutex<Scrollback>,
}

impl Terminal {
    /// Types `data` into it.
    pub fn input(&self, data: &[u8]) -> Result<(), String> {
        let mut writer = self.writer.lock().expect("terminal writer lock");
        writer
            .write_all(data)
            .and_then(|()| writer.flush())
            .map_err(|e| format!("terminal {}: {e}", self.id))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master
            .lock()
            .expect("terminal pty lock")
            .resize(size(cols, rows))
            .map_err(|e| format!("terminal {}: {e}", self.id))
    }

    /// Its scrollback, as bytes.
    pub fn scrollback(&self) -> Vec<u8> {
        self.scrollback.lock().expect("scrollback lock").bytes()
    }

    /// Its last `lines` lines as text.
    pub fn text(&self, lines: usize) -> String {
        self.scrollback.lock().expect("scrollback lock").text(lines)
    }

    pub(crate) fn kill(&self) {
        let _ = self.child.lock().expect("terminal child lock").kill();
    }
}

pub(crate) fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.clamp(2, 500),
        cols: cols.clamp(2, 1000),
        pixel_width: 0,
        pixel_height: 0,
    }
}
