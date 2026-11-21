use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where the bytes for the program go: called with each write.
pub type Sink = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

/// How long after a replay what the model writes is dropped: its answers to what the old output
/// asked. (The model writes on a thread of its own, so they arrive a moment after the replay.)
const REPLAY_QUIET: Duration = Duration::from_millis(100);

/// What the terminal model writes to its program (encoded keys, pastes, answers to queries).
/// wezterm-term writes on a thread of its own, so the bytes arrive a moment later: given to a
/// sink as they come, or kept for [`Output::take`].
#[derive(Clone, Default)]
pub struct Output {
    bytes: Arc<Mutex<Vec<u8>>>,
    sink: Option<Sink>,
    quiet_until: Arc<Mutex<Option<Instant>>>,
}

impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("sink", &self.sink.is_some())
            .finish_non_exhaustive()
    }
}

impl Output {
    pub fn new(sink: Option<Sink>) -> Self {
        Self {
            sink,
            ..Self::default()
        }
    }

    /// Everything written since the last take (without a sink).
    pub fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.bytes.lock().expect("output lock"))
    }

    /// Drops what is written for a moment from now (after a replay).
    pub fn quiet(&self) {
        *self.quiet_until.lock().expect("output lock") = Some(Instant::now() + REPLAY_QUIET);
    }

    /// Writes go through again: what comes next is the user's (a key, a paste).
    pub fn unquiet(&self) {
        *self.quiet_until.lock().expect("output lock") = None;
    }
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let quiet = self
            .quiet_until
            .lock()
            .expect("output lock")
            .is_some_and(|until| Instant::now() < until);
        if quiet {
            return Ok(buf.len());
        }
        match &self.sink {
            Some(sink) => sink(buf.to_vec()),
            None => self
                .bytes
                .lock()
                .expect("output lock")
                .extend_from_slice(buf),
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
