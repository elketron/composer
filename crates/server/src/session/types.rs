use std::fs::File;

use composer_protocol::Seq;

/// Mutable part of a transcript, behind its lock.
#[derive(Debug)]
pub struct TranscriptInner {
    pub next_seq: Seq,
    pub file: File,
}
