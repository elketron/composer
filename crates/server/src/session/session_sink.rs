use std::sync::Arc;

use composer_protocol::{Seq, ServerMessage, TurnState};
use serde_json::Value;

use crate::hub::EventHub;
use crate::provider::ModelMessage;
use crate::session::{RunSink, Session};

/// A stored session as a run's sink: events go to its transcript and its clients, history to its
/// folder. The result is already in its transcript (the last answer, or the `finish` call).
pub struct SessionSink {
    session: Arc<Session>,
    hub: EventHub,
}

impl SessionSink {
    pub fn new(session: Arc<Session>, hub: EventHub) -> Self {
        Self { session, hub }
    }
}

impl RunSink for SessionSink {
    fn id(&self) -> &str {
        self.session.id()
    }

    fn emit(&self, make: Box<dyn FnOnce(Seq) -> ServerMessage + Send + '_>) -> Seq {
        self.session.emit(&self.hub, make)
    }

    fn set_state(&self, state: TurnState) {
        self.session
            .update_info(&self.hub, |info| info.state = state);
    }

    fn set_context_tokens(&self, tokens: u32) {
        self.session
            .update_info(&self.hub, |info| info.context_tokens = tokens);
    }

    fn history(&self) -> Vec<ModelMessage> {
        self.session.history()
    }

    fn push_history(&self, message: ModelMessage) -> Result<(), String> {
        self.session
            .push_history(message)
            .map_err(|e| e.to_string())
    }

    fn site(&self) -> Option<String> {
        self.session.info().site
    }

    fn set_site(&self, site: Option<String>) {
        self.session.update_info(&self.hub, |info| info.site = site);
    }

    fn finish(&self, _value: &Value) {}

    fn session(&self) -> Option<Arc<Session>> {
        Some(self.session.clone())
    }
}
