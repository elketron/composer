use std::collections::HashMap;
use std::sync::Mutex;

use composer_protocol::{ApprovalDecision, ServerMessage};
use tokio::sync::oneshot;

/// Approval requests waiting for an answer. Kept so clients that attach later see them.
#[derive(Default)]
pub struct PendingApprovals {
    waiting: Mutex<HashMap<String, (ServerMessage, oneshot::Sender<ApprovalDecision>)>>,
}

impl PendingApprovals {
    /// Registers a request; the receiver resolves when someone answers.
    pub fn add(&self, id: &str, request: ServerMessage) -> oneshot::Receiver<ApprovalDecision> {
        let (tx, rx) = oneshot::channel();
        self.waiting
            .lock()
            .expect("pending lock")
            .insert(id.to_string(), (request, tx));
        rx
    }

    /// Answers a request. False if it is unknown or already answered.
    pub fn resolve(&self, id: &str, decision: ApprovalDecision) -> bool {
        match self.waiting.lock().expect("pending lock").remove(id) {
            Some((_, tx)) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    pub fn remove(&self, id: &str) {
        self.waiting.lock().expect("pending lock").remove(id);
    }

    /// All open requests (sent to every client when it connects).
    pub fn all(&self) -> Vec<ServerMessage> {
        self.waiting
            .lock()
            .expect("pending lock")
            .values()
            .map(|(m, _)| m.clone())
            .collect()
    }

    /// Open requests of one session (sent to a client when it attaches).
    pub fn for_session(&self, session: &str) -> Vec<ServerMessage> {
        self.waiting
            .lock()
            .expect("pending lock")
            .values()
            .filter(|(m, _)| matches!(m, ServerMessage::ApprovalRequest { session: s, .. } if s == session))
            .map(|(m, _)| m.clone())
            .collect()
    }
}
