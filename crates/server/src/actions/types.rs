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
}

/// One approved recipe version (`state/approvals.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    pub site: String,
    pub recipe: String,
    pub hash: String,
    /// Set (`app`) on approvals given to apps before they were removed: never counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
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

/// Why a recipe was not approved to run.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// Denied by the user, or the question was cancelled.
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
