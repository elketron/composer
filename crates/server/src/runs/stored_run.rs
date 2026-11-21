use std::sync::Arc;

use composer_protocol::{ChatMessage, MessageSource, Role, ServerMessage, SessionId};
use serde_json::Value;
use tokio::sync::watch;

use crate::runs::RunRequest;
use crate::server::Server;
use crate::session::{Session, TurnEnd, submit};

/// The longest part of a child's answer posted into its parent.
const PARENT_NOTE_MAX: usize = 2000;

/// A run stored as a session: started, and its first turn's end to wait for.
pub struct StoredRun {
    pub session: Arc<Session>,
    ends: watch::Receiver<TurnEnd>,
}

impl StoredRun {
    /// Starts `request` as a new session (spawned by `parent`, if given) and sends it its input.
    /// `waited`: whoever starts it waits for [`StoredRun::result`], so the parent is not told
    /// when that first turn ends.
    ///
    /// Every run here is on composer's own harness. An agentic run on an outside agent (ACP)
    /// starts here too, once that harness exists: the request picks it, and the session
    /// records it.
    pub fn start(
        server: &Arc<Server>,
        request: &RunRequest,
        parent: Option<SessionId>,
        waited: bool,
    ) -> Result<Self, String> {
        let session = server
            .sessions
            .create_run(
                request.kind,
                None,
                request.site.clone(),
                request.folder.clone(),
                parent,
            )
            .map_err(|e| e.to_string())?;
        server.hub.send(ServerMessage::SessionUpdated {
            session: session.info(),
        });
        let ends = session.turn_ends();
        session.set_parent_waiting(waited);
        let message = ChatMessage {
            role: Role::User,
            content: request.input.clone(),
            thinking: None,
            tool_calls: vec![],
            pages: vec![],
            images: vec![],
            attachments: vec![],
            source: Some(MessageSource::Server),
        };
        submit(server, &session, message);
        Ok(Self { session, ends })
    }

    /// The result of its first turn, once it has ended.
    pub async fn result(mut self) -> Result<Value, String> {
        let end = self
            .ends
            .wait_for(|end| end.count > 0)
            .await
            .map_err(|_| "the session went away".to_string())?;
        end.result
            .clone()
            .unwrap_or_else(|| Err("the turn ended without a result".into()))
    }
}

/// A spawned session's turn ended: a short note with its last answer goes into the session
/// that spawned it, which starts (or queues) a turn there.
pub fn tell_parent(server: &Arc<Server>, child: &Session, outcome: &Result<Value, String>) {
    let info = child.info();
    let Some(parent) = info.parent.as_deref() else {
        return;
    };
    let Ok(parent) = server.sessions.get(parent) else {
        return;
    };
    let content = match outcome {
        Ok(Value::String(answer)) => format!(
            "[spawned run {} ended] {}",
            info.id,
            shortened(answer, PARENT_NOTE_MAX)
        ),
        Ok(value) => format!(
            "[spawned run {} ended] {}",
            info.id,
            shortened(&value.to_string(), PARENT_NOTE_MAX)
        ),
        Err(error) => format!("[spawned run {} stopped] {error}", info.id),
    };
    let message = ChatMessage {
        role: Role::User,
        content,
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        attachments: vec![],
        source: Some(MessageSource::Server),
    };
    submit(server, &parent, message);
}

fn shortened(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}
