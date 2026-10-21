/// How a stage uses one of its inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// Input to transform (an earlier stage's output, the brief).
    Working,
    /// Rules to follow (`_config/voice.md`).
    Reference,
}

/// A row of a stage contract's Inputs table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageInput {
    pub kind: InputKind,
    /// Relative to the work (`01_spec/spec.md`, `brief`) or the pipeline (`_config/…`).
    pub path: String,
    /// What the stage reads it for.
    pub about: String,
}

/// Stage states that let the next stage start.
pub const FINISHED: [&str; 2] = ["done", "approved"];
