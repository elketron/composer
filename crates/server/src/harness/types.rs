use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use composer_protocol::ChatMessage;

use crate::attachments::TicketFile;
use crate::config_sections::ProjectSettings;
use crate::server::Server;

/// What a harness gets to work a ticket. The worker session already exists and the ticket
/// is claimed.
#[derive(Debug, Clone)]
pub struct StartRequest {
    /// The worker session: the run's events go there.
    pub session: String,
    pub ticket: String,
    pub agent: String,
    /// The folder to work in (the ticket's worktree, or its project folder).
    pub project: Option<String>,
    /// The project's settings in config.toml.
    pub settings: ProjectSettings,
    /// The model to run (allowed for the project); `None`: the harness's default.
    pub model: Option<String>,
    /// Files the ticket's `attachments:` refer to (checked: same project).
    pub files: Vec<TicketFile>,
    /// The run's own output: `state/workers/<session>.log` (appended to).
    pub log: PathBuf,
    /// Continue an earlier run of this ticket (the harness's own session id) with a message
    /// from the user, instead of starting fresh.
    pub resume: Option<Resume>,
}

#[derive(Debug, Clone)]
pub struct Resume {
    pub harness_session: String,
    pub message: String,
    /// Files attached to the message (in the worker session's attachments folder).
    pub files: Vec<PathBuf>,
}

/// Why a run stopped without finishing, to be picked up later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pause {
    pub reason: String,
    /// When it can go on (unix seconds), if known.
    pub until: Option<i64>,
}

/// Something that works tickets: composer's own agent loop, or an external CLI.
#[async_trait]
pub trait Harness: Send + Sync {
    /// The name in a ticket's `harness:`.
    fn name(&self) -> &'static str;

    /// External harnesses run only in projects that allow them (`external_harnesses`).
    fn external(&self) -> bool {
        true
    }

    /// The model it runs unless a project's `models` say otherwise (`None`: its own default).
    fn default_model(&self, server: &Server) -> Option<String>;

    /// Whether it can run another model than its default.
    fn chooses_model(&self) -> bool {
        true
    }

    async fn start(
        &self,
        server: &Arc<Server>,
        request: StartRequest,
    ) -> Result<Arc<dyn HarnessRun>, String>;
}

/// One ticket being worked. Its events and `finish` reach the server through a
/// [`WorkerSink`](crate::harness::WorkerSink); a run that ends without `finish` fails the ticket.
#[async_trait]
pub trait HarnessRun: Send + Sync {
    /// Resolves once the run has ended, with how it ended ("exit 0", "killed").
    async fn wait(&self) -> String;

    /// Stops the run and everything it started.
    fn interrupt(&self);

    /// A message from the user while the run works.
    fn send(&self, message: ChatMessage) -> Result<(), String> {
        let _ = message;
        Err(
            "this worker can't take messages while it works; cancel it, or tell the orchestrator"
                .into(),
        )
    }

    /// Set when the run stopped to be resumed later (a usage limit), rather than failed.
    fn paused(&self) -> Option<Pause> {
        None
    }
}
