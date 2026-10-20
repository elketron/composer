use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use composer_protocol::{ClientCommand, CommandResult};
use tokio::sync::oneshot;

use crate::harness::HarnessRun;

/// A command for the parent, with where its reply goes (`None`: no reply wanted).
pub struct Outbound {
    pub command: ClientCommand,
    pub reply: Option<oneshot::Sender<Result<CommandResult, String>>>,
}

/// A worker this server started.
#[derive(Clone)]
pub struct WorkerHandle {
    pub ticket: String,
    /// The orchestrator session that spawned it.
    pub parent: String,
    /// The harness working the ticket (`composer`, …).
    pub harness: &'static str,
    pub run: Arc<dyn HarnessRun>,
    /// Set when the user cancels it, so its end reads as cancelled rather than a crash.
    pub cancelled: Arc<AtomicBool>,
}
