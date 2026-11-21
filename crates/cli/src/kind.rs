use composer_protocol::RunKind;

/// A run kind, as the command line takes it.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    /// One model call without tools.
    Single,
    /// Read-only tools; ends with `finish(value)`.
    Workflow,
    /// A conversation with every tool.
    Agentic,
}

impl From<Kind> for RunKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Single => RunKind::SingleShot,
            Kind::Workflow => RunKind::Workflow,
            Kind::Agentic => RunKind::Agentic,
        }
    }
}
