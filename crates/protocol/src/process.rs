//! A session's background processes: commands the agent started (`process_start`) that run in
//! a terminal of their own until the agent or the user stops them, or the server stops.

use serde::{Deserialize, Serialize};

/// A background process that is running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProcessInfo {
    /// The terminal it runs in (`terminal.read` / `terminal.output`, `process.stop`).
    pub terminal: String,
    /// The command, as the agent gave it.
    pub command: String,
    /// The folder it runs in.
    pub cwd: String,
    /// When it started, RFC 3339.
    pub started: String,
}
