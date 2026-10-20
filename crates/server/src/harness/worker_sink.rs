use std::sync::Arc;

use composer_protocol::{ServerMessage, TurnState, WorkerState};

use crate::harness::resequence::resequence;
use crate::server::Server;
use crate::session::Session;

/// Where a harness run reports to: the worker session on this server, which every client sees.
pub struct WorkerSink {
    server: Arc<Server>,
    session: Arc<Session>,
}

impl WorkerSink {
    pub fn new(server: &Arc<Server>, session: &str) -> Option<Self> {
        let session = server.sessions.get(session).ok()?;
        Some(Self {
            server: server.clone(),
            session,
        })
    }

    /// An event of the run's session: re-sequenced into the worker session here, or (a
    /// `SessionUpdated`) its state, context size and project mirrored.
    pub fn event(&self, event: ServerMessage) {
        match event {
            ServerMessage::SessionUpdated { session: info } => {
                self.session.update_info(&self.server.hub, |mine| {
                    // Finished (`finish`, or the run ended): late updates from the run must
                    // not make it look busy again.
                    let finished =
                        matches!(mine.worker, Some(WorkerState::Done | WorkerState::Failed));
                    if !finished {
                        mine.state = info.state;
                    }
                    mine.context_tokens = info.context_tokens;
                    mine.project = info.project;
                })
            }
            event if resequence(&event, 0).is_some() => {
                self.session.emit(&self.server.hub, |seq| {
                    resequence(&event, seq).expect("checked above")
                });
            }
            _ => {}
        }
    }

    /// The run's turn state (external harnesses; own workers send `SessionUpdated`).
    pub fn set_state(&self, state: TurnState) {
        let mut info = self.session.info();
        info.state = state;
        self.event(ServerMessage::SessionUpdated { session: info });
    }

    pub fn session(&self) -> &str {
        self.session.id()
    }

    /// The run is done with its ticket.
    pub fn finish(&self, state: WorkerState, summary: String) {
        self.server
            .workers
            .finish(&self.server, self.session.id(), state, summary);
    }
}
