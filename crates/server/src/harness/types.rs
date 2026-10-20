use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use composer_protocol::ChatMessage;

use crate::server::Server;

/// What a harness gets to work a ticket. The worker session already exists and the ticket
/// is claimed.
#[derive(Debug, Clone)]
pub struct StartRequest {
    /// The worker session: the run's events go there.
    pub session: String,
    pub ticket: String,
    pub agent: String,
    /// The folder to work in, if the ticket has a project.
    pub project: Option<String>,
    /// The run's own output: `state/workers/<session>.log`.
    pub log: PathBuf,
}

/// Something that works tickets: composer's own agent loop, or an external CLI.
pub trait Harness: Send + Sync {
    /// The name in a ticket's `harness:`.
    fn name(&self) -> &'static str;

    fn start(
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
}
