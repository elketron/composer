use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use composer_protocol::{Seq, ServerMessage, TurnState};
use serde_json::Value;

use crate::provider::ModelMessage;
use crate::session::{RunSink, Session};

/// A run that is not stored: its events, history and result stay in memory, and it needs no
/// session folder.
pub struct MemorySink {
    id: String,
    next_seq: AtomicU64,
    events: Mutex<Vec<ServerMessage>>,
    history: Mutex<Vec<ModelMessage>>,
    site: Mutex<Option<String>>,
    result: Mutex<Option<Value>>,
    aborted: Mutex<Option<String>>,
}

impl MemorySink {
    /// A run whose input is `input`, in `site` if given.
    pub fn new(input: &str, site: Option<String>) -> Self {
        Self {
            id: format!("run-{:08x}", rand::random::<u32>()),
            next_seq: AtomicU64::new(1),
            events: Mutex::new(Vec::new()),
            history: Mutex::new(vec![ModelMessage::User {
                content: input.to_string(),
                images: vec![],
            }]),
            site: Mutex::new(site),
            result: Mutex::new(None),
            aborted: Mutex::new(None),
        }
    }

    /// What the run did, in order.
    pub fn events(&self) -> Vec<ServerMessage> {
        self.events.lock().expect("events lock").clone()
    }

    /// Its result, once it has finished.
    pub fn result(&self) -> Option<Value> {
        self.result.lock().expect("result lock").clone()
    }
}

impl RunSink for MemorySink {
    fn id(&self) -> &str {
        &self.id
    }

    fn emit(&self, make: Box<dyn FnOnce(Seq) -> ServerMessage + Send + '_>) -> Seq {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        self.events.lock().expect("events lock").push(make(seq));
        seq
    }

    fn set_state(&self, _state: TurnState) {}

    fn set_context_tokens(&self, _tokens: u32) {}

    fn history(&self) -> Vec<ModelMessage> {
        self.history.lock().expect("history lock").clone()
    }

    fn push_history(&self, message: ModelMessage) -> Result<(), String> {
        self.history.lock().expect("history lock").push(message);
        Ok(())
    }

    fn site(&self) -> Option<String> {
        self.site.lock().expect("site lock").clone()
    }

    fn set_site(&self, site: Option<String>) {
        *self.site.lock().expect("site lock") = site;
    }

    fn finish(&self, value: &Value) {
        *self.result.lock().expect("result lock") = Some(value.clone());
    }

    fn abort(&self, error: String) {
        self.aborted
            .lock()
            .expect("aborted lock")
            .get_or_insert(error);
    }

    fn aborted(&self) -> Option<String> {
        self.aborted.lock().expect("aborted lock").clone()
    }

    fn session(&self) -> Option<Arc<Session>> {
        None
    }
}
