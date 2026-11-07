/// A terminal as a client keeps it: its recent bytes (for drawing from scratch) and whether it
/// ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalView {
    pub scrollback: Vec<u8>,
    pub exited: bool,
}

/// Bytes kept per terminal.
const KEEP: usize = 256 * 1024;

impl TerminalView {
    pub fn push(&mut self, bytes: &[u8]) {
        self.scrollback.extend_from_slice(bytes);
        let over = self.scrollback.len().saturating_sub(KEEP);
        self.scrollback.drain(..over);
    }
}
