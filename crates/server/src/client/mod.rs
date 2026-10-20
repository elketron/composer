//! Minimal command-line clients of the server, until the TUI exists.

mod ask;
mod harness;

pub use ask::{AskOptions, ask};
pub use harness::harness;
