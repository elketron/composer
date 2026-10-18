/// Incremental server-sent-events decoder: feed it bytes, get back `data:` payloads.
///
/// Handles events split across chunks (including inside UTF-8 sequences),
/// `\r\n` line endings, multi-line data and `:` comment lines (keep-alives).
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the payloads of all events completed by `chunk`.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.buffer.drain(..=newline).collect();
            let line = String::from_utf8_lossy(&raw);
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data
                    .push(value.strip_prefix(' ').unwrap_or(value).to_string());
            }
            // `:` comments and other fields (event:, id:, retry:) are ignored.
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_split_events_and_skips_comments() {
        let mut d = SseDecoder::new();
        assert!(d.push(b": keep-alive\n\ndata: {\"a\"").is_empty());
        assert_eq!(
            d.push(b":1}\r\n\r\ndata: [DONE]\n\n"),
            ["{\"a\":1}", "[DONE]"]
        );
    }

    #[test]
    fn utf8_split_across_chunks() {
        let mut d = SseDecoder::new();
        let bytes = "data: héllo\n\n".as_bytes();
        assert!(d.push(&bytes[..8]).is_empty());
        assert_eq!(d.push(&bytes[8..]), ["héllo"]);
    }

    #[test]
    fn multi_line_data() {
        let mut d = SseDecoder::new();
        assert_eq!(d.push(b"data: a\ndata: b\n\n"), ["a\nb"]);
    }
}
