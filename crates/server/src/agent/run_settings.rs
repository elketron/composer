use composer_protocol::{RunKind, SessionInfo};

/// What every run is told first, whatever its kind.
const BASE_PROMPT: &str = "You are composer's agent. You work with the user in their web of \
pages (sites, `site://`) and in their projects. Open and search pages before you answer from \
memory; write what lasts (decisions, facts about the user) into their pages. Keep answers short.";

/// A workflow run's own rules.
const WORKFLOW_PROMPT: &str = "This is a workflow run: you can read pages but not change \
anything. When you have the answer, call `finish` with it as `value`; that ends the run.";

/// A single shot's own rules.
const SINGLE_SHOT_PROMPT: &str = "This is a single shot: you have no tools. Answer the input \
in one reply.";

/// How a run goes: its kind, the model and how much it thinks, and its instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSettings {
    pub kind: RunKind,
    /// `None`: the provider's model.
    pub model: Option<String>,
    /// The system prompt's first section.
    pub prompt: String,
    /// `reasoning_effort` (none, low, medium, high); `None`: `[agent] reasoning_effort`, else
    /// the provider's own.
    pub reasoning: Option<String>,
}

impl RunSettings {
    /// The default settings of a run of `kind`.
    pub fn new(kind: RunKind) -> Self {
        let prompt = match kind {
            RunKind::SingleShot => format!("{BASE_PROMPT}\n\n{SINGLE_SHOT_PROMPT}"),
            RunKind::Workflow => format!("{BASE_PROMPT}\n\n{WORKFLOW_PROMPT}"),
            RunKind::Agentic => BASE_PROMPT.to_string(),
        };
        Self {
            kind,
            model: None,
            prompt,
            reasoning: None,
        }
    }

    /// A session's runs: its kind and the model it was set to.
    pub fn for_session(info: &SessionInfo) -> Self {
        Self {
            model: info.model.clone(),
            ..Self::new(info.kind)
        }
    }
}
