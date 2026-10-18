use tokio::sync::mpsc;

use crate::action::Action;

/// The single entry point for actions: keys, server events and effect results all
/// go through here, and the app loop feeds them to the stores in order.
#[derive(Debug, Clone)]
pub struct Dispatcher {
    tx: mpsc::UnboundedSender<Action>,
}

impl Dispatcher {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Action>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub fn dispatch(&self, action: Action) {
        let _ = self.tx.send(action);
    }
}
