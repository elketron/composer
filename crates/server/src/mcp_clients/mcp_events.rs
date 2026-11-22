//! Events from MCP servers (`notifications/composer/event`) delivered into sessions as automatic
//! notices, so an app the user works in can tell the agent what they did.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use composer_protocol::{ChatMessage, MessageSource, Role};
use serde_json::Value;

use crate::mcp_clients::McpEvent;
use crate::server::Server;
use crate::session::submit;

/// A session gets at most one notice per server in this long; events that come meanwhile go
/// together in the next one.
pub const COALESCE: Duration = Duration::from_secs(1);

/// What one server (by connection) has for one session.
#[derive(Default)]
struct Outbox {
    /// When it was last told.
    told: Option<Instant>,
    /// Waiting for the next notice, with the name the session knows the server by.
    waiting: Vec<McpEvent>,
    name: String,
}

/// Routes servers' events to sessions. Connections are told apart by their pool key (what they
/// start or reach): one process may serve sessions in several sites.
pub struct McpEvents {
    server: Weak<Server>,
    /// The session that called a tool of each connection last.
    last_caller: Mutex<HashMap<String, String>>,
    outboxes: Mutex<HashMap<(String, String), Outbox>>,
}

impl McpEvents {
    pub fn new(server: Weak<Server>) -> Self {
        Self {
            server,
            last_caller: Mutex::default(),
            outboxes: Mutex::default(),
        }
    }

    /// `session` is calling a tool of connection `key`.
    pub fn called(&self, key: &str, session: &str) {
        self.last_caller
            .lock()
            .expect("callers lock")
            .insert(key.to_string(), session.to_string());
    }

    /// Connection `key` (started as `name`) sent `params`: the session it names, else the one
    /// that called it last, gets it, if that session can use the server. Otherwise it is dropped
    /// with a log line.
    pub fn received(self: &Arc<Self>, key: &str, name: &str, params: &Value) {
        let event = match McpEvent::parse(params) {
            Ok(event) => event,
            Err(e) => {
                tracing::warn!("mcp {name}: an event dropped: {e}");
                return;
            }
        };
        let Some(server) = self.server.upgrade() else {
            return;
        };
        let session = match &event.session {
            Some(session) => session.clone(),
            None => match self.last_caller.lock().expect("callers lock").get(key) {
                Some(session) => session.clone(),
                None => {
                    tracing::warn!(
                        "mcp {name}: an event dropped: it names no session and none called it yet"
                    );
                    return;
                }
            },
        };
        let Some(known_as) = reaching(&server, key, &session) else {
            tracing::warn!(
                "mcp {name}: an event for session {session} dropped: no such session, or it \
                 can't use this server"
            );
            return;
        };
        self.queue(key, &session, &known_as, event);
    }

    /// Tells the session now, or with the next notice when it was told a moment ago.
    fn queue(self: &Arc<Self>, key: &str, session: &str, name: &str, event: McpEvent) {
        let mut outboxes = self.outboxes.lock().expect("outboxes lock");
        outboxes.retain(|_, outbox| {
            !outbox.waiting.is_empty() || outbox.told.is_some_and(|at| at.elapsed() < COALESCE)
        });
        let id = (key.to_string(), session.to_string());
        let outbox = outboxes.entry(id.clone()).or_default();
        outbox.name = name.to_string();
        if !outbox.waiting.is_empty() {
            outbox.waiting.push(event);
            return;
        }
        let wait = outbox
            .told
            .map(|at| COALESCE.saturating_sub(at.elapsed()))
            .unwrap_or_default();
        if wait.is_zero() {
            outbox.told = Some(Instant::now());
            drop(outboxes);
            self.deliver(session, name, &[event]);
            return;
        }
        outbox.waiting.push(event);
        drop(outboxes);
        let events = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            events.flush(&id);
        });
    }

    fn flush(&self, id: &(String, String)) {
        let mut outboxes = self.outboxes.lock().expect("outboxes lock");
        let Some(outbox) = outboxes.get_mut(id) else {
            return;
        };
        let waiting = std::mem::take(&mut outbox.waiting);
        let name = outbox.name.clone();
        outbox.told = Some(Instant::now());
        drop(outboxes);
        if !waiting.is_empty() {
            self.deliver(&id.1, &name, &waiting);
        }
    }

    /// The notice into the session: it wakes an idle agent, or waits behind the running turn.
    fn deliver(&self, session: &str, server_name: &str, events: &[McpEvent]) {
        let Some(server) = self.server.upgrade() else {
            return;
        };
        let Ok(session) = server.sessions.get(session) else {
            tracing::warn!("mcp {server_name}: session {session} is gone; its events dropped");
            return;
        };
        let message = ChatMessage {
            role: Role::User,
            content: McpEvent::notice(server_name, events),
            thinking: None,
            tool_calls: vec![],
            pages: vec![],
            images: vec![],
            attachments: vec![],
            source: Some(MessageSource::Mcp {
                server: server_name.to_string(),
            }),
        };
        submit(&server, &session, message);
    }
}

/// The name `session` knows connection `key` by, when it is a session that has the server's
/// tools: the server is `config.toml`'s, or its site's.
fn reaching(server: &Server, key: &str, session: &str) -> Option<String> {
    let session = server.sessions.get(session).ok()?;
    let info = session.info();
    let site = info
        .site
        .as_deref()
        .and_then(|site| crate::site_mcp::active(server, site));
    server.mcp().reaching(info.kind, site.as_deref(), key)
}
