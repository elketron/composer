use serde::{Deserialize, Serialize};

/// A recipe as the agent sees it in `actions`.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub name: String,
    pub doc: Option<String>,
    /// Parameter names, `name=default` or `*name` for variadics.
    pub parameters: Vec<String>,
    /// `[private]` or `_`-prefixed: helpers, not runnable by the agent.
    pub private: bool,
}

/// One approved recipe version (`state/approvals.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    pub site: String,
    pub recipe: String,
    pub hash: String,
    pub approved_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    /// `None` when killed (timeout or cancel).
    pub exit_code: Option<i32>,
    /// stdout and stderr in the order produced (one pipe), trimmed to a size the model can take.
    pub output: String,
    pub timed_out: bool,
}
