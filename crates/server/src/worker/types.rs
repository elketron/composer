use composer_protocol::{ClientCommand, CommandResult};
use tokio::sync::oneshot;

/// A command for the parent, with where its reply goes (`None`: no reply wanted).
pub struct Outbound {
    pub command: ClientCommand,
    pub reply: Option<oneshot::Sender<Result<CommandResult, String>>>,
}

/// A worker process the parent started.
#[derive(Debug, Clone)]
pub struct WorkerHandle {
    pub ticket: String,
    /// The orchestrator session that spawned it.
    pub parent: String,
}
