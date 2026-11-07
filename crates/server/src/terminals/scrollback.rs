use std::collections::VecDeque;

/// A terminal's recent output, bounded: the bytes for a client that joins late, and plain text
/// (escape sequences stripped, carriage returns applied) for the agent.
#[derive(Debug, Clone)]
pub struct Scrollback {
    bytes: VecDeque<u8>,
    cap: usize,
}

impl Scrollback {
    pub fn new(cap: usize) -> Self {
        Self {
            bytes: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.bytes.extend(data);
        let over = self.bytes.len().saturating_sub(self.cap);
        self.bytes.drain(..over);
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.iter().copied().collect()
    }

    /// The last `lines` lines as the screen shows them.
    pub fn text(&self, lines: usize) -> String {
        let text = strip(&String::from_utf8_lossy(&self.bytes()));
        let shown: Vec<String> = text
            .trim_end_matches(['\n', '\r'])
            .split('\n')
            .map(|line| {
                line.rsplit('\r')
                    .find(|part| !part.is_empty())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        let start = shown.len().saturating_sub(lines);
        shown[start..].join("\n").trim_end().to_string()
    }
}

/// `text` without terminal escape sequences (CSI, OSC, and the two-byte ones) or other controls
/// but newlines, carriage returns and tabs.
fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            if !c.is_control() || matches!(c, '\n' | '\r' | '\t') {
                out.push(c);
            }
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte in @..~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_and_readable() {
        let mut scroll = Scrollback::new(10);
        scroll.push(b"0123456789abc");
        assert_eq!(scroll.bytes(), b"3456789abc");
        let mut scroll = Scrollback::new(1000);
        scroll.push(
            b"\x1b[1;32mok\x1b[0m done\r\nprogress 10%\rprogress 100%\r\n\x1b]0;title\x07$ ls\r\n",
        );
        assert_eq!(scroll.text(10), "ok done\nprogress 100%\n$ ls");
        assert_eq!(scroll.text(1), "$ ls");
    }
}
