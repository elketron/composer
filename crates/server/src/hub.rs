use std::sync::Arc;

use composer_protocol::ServerMessage;
use tokio::sync::broadcast;

/// Fan-out of every server event to all connections; each connection filters
/// for what its client attached to or subscribed to.
#[derive(Debug, Clone)]
pub struct EventHub {
    tx: broadcast::Sender<Arc<ServerMessage>>,
}

impl EventHub {
    pub fn new(capacity: usize) -> Self {
        Self {
            tx: broadcast::channel(capacity).0,
        }
    }

    pub fn send(&self, message: ServerMessage) {
        // No receivers is fine: nobody is connected.
        let _ = self.tx.send(Arc::new(message));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<ServerMessage>> {
        self.tx.subscribe()
    }
}
