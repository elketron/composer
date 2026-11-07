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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_bytes_only() {
        let mut view = TerminalView::default();
        view.push(&vec![b'a'; KEEP]);
        view.push(b"tail");
        assert_eq!(view.scrollback.len(), KEEP);
        assert!(view.scrollback.ends_with(b"atail"));
    }
}
