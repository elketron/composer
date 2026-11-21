use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use portable_pty::{ChildKiller, MasterPty, PtySize};

use crate::terminals::Scrollback;
use crate::terminals::types::{Background, StoppedBy};

/// One server-owned terminal of a session (ticket 0176): the user's shell (or a program, or a
/// background process the agent started) on a PTY and its recent output.
pub struct Terminal {
    pub id: String,
    /// The session it belongs to.
    pub session: String,
    pub cwd: PathBuf,
    pub(crate) master: Mutex<Box<dyn MasterPty + Send>>,
    pub(crate) writer: Mutex<Box<dyn Write + Send>>,
    pub(crate) killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    /// Its process, which leads its own process group (the PTY makes it a session leader).
    pub(crate) pid: Option<u32>,
    pub(crate) scrollback: Mutex<Scrollback>,
    /// What it runs, when it is a background process (`process_start`).
    pub background: Option<Background>,
    /// Who stopped it, once someone did.
    pub(crate) stopped: Mutex<Option<StoppedBy>>,
    /// Its output has ended (the PTY closed).
    pub(crate) drained: AtomicBool,
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

    /// Its process (the leader of its process group).
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Who stopped it, if anyone did.
    pub fn stopped_by(&self) -> Option<StoppedBy> {
        *self.stopped.lock().expect("terminal stopped lock")
    }

    /// Ends it: its whole process group, asked first (`SIGHUP`, `SIGTERM`), then killed after
    /// `grace`. The first to stop it is the one who did.
    pub(crate) fn stop(&self, by: StoppedBy, grace: std::time::Duration) {
        self.stopped
            .lock()
            .expect("terminal stopped lock")
            .get_or_insert(by);
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            crate::harness::signal_group(pid, libc::SIGHUP);
            crate::harness::signal_group(pid, libc::SIGTERM);
            std::thread::spawn(move || {
                std::thread::sleep(grace);
                crate::harness::signal_group(pid, libc::SIGKILL);
            });
            return;
        }
        let _ = grace;
        self.kill();
    }

    pub(crate) fn kill(&self) {
        let _ = self.killer.lock().expect("terminal killer lock").kill();
    }

    /// Whether anything of its process group is still alive.
    #[cfg(unix)]
    pub(crate) fn group_alive(&self) -> bool {
        // SAFETY: signal 0 only checks that the group exists.
        self.pid
            .is_some_and(|pid| unsafe { libc::killpg(pid as libc::pid_t, 0) } == 0)
    }

    pub(crate) fn set_drained(&self) {
        self.drained.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_drained(&self) -> bool {
        self.drained.load(Ordering::SeqCst)
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
