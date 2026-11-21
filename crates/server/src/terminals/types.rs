//! Plain data about terminals.

/// A background process: the command the agent started and when.
#[derive(Debug, Clone)]
pub struct Background {
    pub command: String,
    /// RFC 3339.
    pub started: String,
}

/// Who stopped a background process: the agent (its tool result says so: no message), the user
/// (the agent is told), or the server shutting down (no one to tell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoppedBy {
    Agent,
    User,
    Server,
}
