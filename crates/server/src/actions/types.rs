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
    /// How many parameters it declares (a variadic counts as one).
    pub parameter_count: usize,
    /// The last parameter takes any number of arguments.
    pub variadic: bool,
    /// The body (or a dependency's arguments) interpolates a parameter as `{{name}}`, which pastes
    /// the raw argument into the shell. The safe forms are `"$1"` with positional arguments, or
    /// an exported parameter (`$name`).
    pub interpolates_parameters: bool,
    /// Marked `[metadata('app')]`: an approval given for agents also counts for apps.
    pub app_visible: bool,
}

/// Who runs a recipe. Approvals don't carry over between them: an agent's `run` is watched in a
/// session and limited by its tools, while an app's HTTP call only needs the token.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Caller {
    /// The `run` tool. Entries written before callers existed are this.
    #[default]
    Agent,
    /// `POST /api/actions/<site>/<recipe>`.
    App,
}

/// One approved recipe version (`state/approvals.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    pub site: String,
    pub recipe: String,
    pub hash: String,
    #[serde(default)]
    pub caller: Caller,
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

/// How an approval was answered. `reason`: its orchestrator decided a worker's approval instead
/// of the user, and why; such an answer holds for this one request.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalAnswer {
    pub decision: composer_protocol::ApprovalDecision,
    pub reason: Option<String>,
}

/// Why a recipe was not approved to run.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// Denied by the user or an orchestrator, or the question was cancelled.
    Denied(String),
    /// The justfile changed while the question was open.
    Changed(String),
}

impl Refusal {
    pub fn message(&self) -> &str {
        match self {
            Refusal::Denied(m) | Refusal::Changed(m) => m,
        }
    }
}
