use std::sync::Arc;

use composer_protocol::{Seq, ServerMessage, TurnState};
use serde_json::Value;

use crate::provider::ModelMessage;
use crate::session::Session;

/// Where a run puts what it does: its events, its history, the site it is in, and its result. A
/// stored session is one ([`crate::session::SessionSink`]); a run that is not stored keeps it all
/// in memory ([`crate::session::MemorySink`]) and needs no session folder.
pub trait RunSink: Send + Sync {
    /// The id its events carry (a session's id, or the run's own).
    fn id(&self) -> &str;
    /// Records an event; `make` gets its seq.
    fn emit(&self, make: Box<dyn FnOnce(Seq) -> ServerMessage + Send + '_>) -> Seq;
    /// What the run is doing now.
    fn set_state(&self, state: TurnState);
    /// Prompt tokens of the last model call: the context in use.
    fn set_context_tokens(&self, tokens: u32);
    /// The conversation so far, as the model sees it.
    fn history(&self) -> Vec<ModelMessage>;
    fn push_history(&self, message: ModelMessage) -> Result<(), String>;
    /// The site the run is in.
    fn site(&self) -> Option<String>;
    fn set_site(&self, site: Option<String>);
    /// The run's result: a workflow run's `finish` value, else its last answer.
    fn finish(&self, value: &Value);
    /// Stops the run after the current tool call with `error`: something needs the user's
    /// approval and nobody can be asked (a run that is not stored).
    fn abort(&self, error: String);
    /// Why the run was stopped, if it was.
    fn aborted(&self) -> Option<String>;
    /// The stored session behind it, for what only a session has (a desk, terminals,
    /// approvals, attachments); `None` for a run that is not stored.
    fn session(&self) -> Option<Arc<Session>>;
}
