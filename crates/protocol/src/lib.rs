//! Messages between the composer server and its clients (TUI, micro apps, workers).
//!
//! WebSocket `/ws`: clients send [`ClientRequest`]s, the server sends [`ServerMessage`]s.
//! Every message is JSON with a `type` field, e.g. `{"type":"session.attach", ...}`.
//! HTTP bodies are in [`http`].
//!
//! With the `ts` feature, `cargo test -p composer-protocol --features ts` writes
//! TypeScript definitions to `crates/protocol/bindings/` for the app sdk.

mod client;
mod common;
pub mod http;
mod server;

pub use client::{ClientCommand, ClientRequest};
pub use common::*;
pub use server::{CommandResult, ServerMessage};
