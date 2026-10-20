//! The main loop of a worker process (`composer worker ...`).

use std::sync::Arc;
use std::time::Duration;

use composer_protocol::{
    ChatMessage, MessageSource, Role, ServerMessage, SessionInfo, TurnState, WorkerState,
};
use tokio::sync::broadcast::error::RecvError;

use crate::config::Config;
use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::init::local_token;
use crate::provider::OpenAiProvider;
use crate::server::Server;
use crate::session::submit;
use crate::worker::ParentLink;

const NUDGE: &str = "You ended your turn without calling `finish`. If the ticket is done, make sure its \
Result section is written, then call finish(done, summary). If you cannot complete it, write why in \
Result and call finish(failed, reason).";

/// What the parent passes on the command line.
#[derive(Debug, Clone)]
pub struct WorkerOptions {
    pub session: String,
    pub ticket: String,
    pub agent: String,
    pub project: Option<String>,
    pub parent_url: String,
}

/// Works the ticket until the agent calls `finish` (or gives up), then returns.
pub async fn run_worker(home: Home, options: WorkerOptions) -> Result<()> {
    let config = Config::load(&home.config_file())?;
    let provider = OpenAiProvider::connect(&config.provider)
        .await
        .map_err(ServerError::Provider)?;
    let token = local_token(&home)?;
    let link = ParentLink::connect(&options.parent_url, &token, &options.session)
        .await
        .map_err(ServerError::Invalid)?;
    let server = Server::new_worker(home, config, Arc::new(provider), link.clone())?;

    let session = server.sessions.create_with_info(SessionInfo {
        id: options.session.clone(),
        agent: options.agent.clone(),
        project: options.project.clone(),
        ticket: Some(options.ticket.clone()),
        parent: None,
        worker: None,
        state: TurnState::Idle,
        last_seq: 0,
        context_tokens: 0,
        preview: None,
        updated: chrono::Utc::now().to_rfc3339(),
    })?;

    // Everything the session does goes to the parent, which shows it to clients.
    let mut events = server.hub.subscribe();
    let mut turns = server.hub.subscribe();
    let forward_link = link.clone();
    let session_id = options.session.clone();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) if belongs_to(&event, &session_id) => {
                    forward_link.event((*event).clone())
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            }
        }
    });

    let ticket_url = format!("site://orchestration/tickets/{}.md", options.ticket);
    submit(
        &server,
        &session,
        user(format!(
            "Your ticket: {ticket_url}\n\nOpen it and the pages in its Context, do the Task, check every \
             \"Done when\" item, write the Result section, then call finish."
        )),
    );

    let mut nudged = false;
    loop {
        tokio::select! {
            event = turns.recv() => match event {
                Ok(event) if matches!(&*event, ServerMessage::TurnDone { session, .. } if *session == options.session) => {
                    if server.worker_finished() {
                        break;
                    }
                    if nudged {
                        let _ = link
                            .finish(WorkerState::Failed, "the worker stopped twice without calling finish".into())
                            .await;
                        break;
                    }
                    nudged = true;
                    submit(&server, &session, user(NUDGE.into()));
                }
                Err(RecvError::Closed) => break,
                _ => {}
            },
            _ = link.closed() => {
                tracing::warn!("lost the parent; stopping");
                return Ok(());
            }
        }
    }
    // Let the last events reach the parent before the process exits.
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}

fn user(content: String) -> ChatMessage {
    ChatMessage {
        role: Role::User,
        content,
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        source: Some(MessageSource::Server),
    }
}

/// Session events and info updates of the worker's own session.
fn belongs_to(event: &ServerMessage, session: &str) -> bool {
    use ServerMessage::*;
    match event {
        TurnStarted { session: s, .. }
        | TurnQueued { session: s, .. }
        | TurnDone { session: s, .. }
        | ThinkingDelta { session: s, .. }
        | MessageDelta { session: s, .. }
        | MessageDone { session: s, .. }
        | ToolCallStarted { session: s, .. }
        | ToolResult { session: s, .. }
        | AgentNavigated { session: s, .. }
        | SessionCompacted { session: s, .. } => s == session,
        SessionUpdated { session: info } => info.id == session,
        _ => false,
    }
}
