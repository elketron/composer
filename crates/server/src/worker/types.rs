use std::sync::{Arc, Mutex};

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
    /// Whether the harness can resume a run later (by the session in the ticket's
    /// `harness_session`).
    pub resumable: bool,
    /// An external CLI (Claude Code): it asks the user through composer's permission tool.
    pub external: bool,
    /// The ticket's project (not its worktree), for the per-project switches.
    pub project: Option<String>,
    /// Where it works: the ticket's worktree, else the project folder.
    pub folder: Option<String>,
    /// The model it runs, when the harness says.
    pub model: Option<String>,
    /// Set when composer stops it, so its end reads as that rather than a crash.
    pub stop: Arc<Mutex<Option<StopReason>>>,
}

/// Why composer stopped a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The user cancelled it.
    Cancelled,
    /// Its harness (or model) was turned off for the project.
    SwitchedOff,
    /// The server is shutting down.
    Shutdown,
}

/// An orchestrator's answer to its worker's approval request (`answer_approval`), with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Approve(String),
    Deny(String),
    /// Not its call: the user is asked, told this.
    AskUser(String),
}
