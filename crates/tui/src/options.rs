use crate::store::types::Startup;

/// How to reach the server and what to open.
#[derive(Debug, Clone)]
pub struct TuiOptions {
    /// e.g. `http://127.0.0.1:7420`
    pub server_url: String,
    pub token: String,
    pub startup: Startup,
    /// Compaction threshold, shown as `ctx 38K/160K`.
    pub context_limit: u32,
}
