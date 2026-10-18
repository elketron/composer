//! The agent loop: user messages in, model calls and tool calls until the model stops.

use std::sync::Arc;

use base64::Engine;
use composer_core::SiteUrl;
use composer_protocol::{ChatMessage, MessageSource, Role, ServerMessage, ToolCall, TurnState};
use tokio_util::sync::CancellationToken;

use crate::agent::{AgentDefinition, system_prompt};
use crate::provider::{ModelEvent, ModelMessage, ModelRequest, ModelToolCall};
use crate::server::Server;
use crate::session::Session;
use crate::tools::ToolContext;

/// Delivers a user-side message. Starts a turn, or queues it behind the running one.
pub fn submit(server: &Arc<Server>, session: &Arc<Session>, message: ChatMessage) {
    let source = message.source.clone().unwrap_or(MessageSource::Tui);
    if session.enqueue(message) {
        tokio::spawn(drive(server.clone(), session.clone()));
    } else {
        session.emit(&server.hub, |seq| ServerMessage::TurnQueued {
            session: session.id().to_string(),
            seq,
            source,
        });
    }
}

/// Runs turns until the inbox is empty.
async fn drive(server: Arc<Server>, session: Arc<Session>) {
    loop {
        let messages = session.take_inbox();
        if messages.is_empty() {
            if session.stop_running() {
                continue;
            }
            return;
        }
        let id = session.id().to_string();
        for message in messages {
            accept_user_message(&server, &session, message);
        }

        let cancel = session.new_turn_token();
        session.emit(&server.hub, |seq| ServerMessage::TurnStarted {
            session: id.clone(),
            seq,
        });
        let outcome = run_turn(&server, &session, &cancel).await;
        if let Err(error) = &outcome {
            tracing::warn!("session {id}: {error}");
            let message = system_message(format!("Turn stopped: {error}"));
            session.emit(&server.hub, |seq| ServerMessage::MessageDone {
                session: id.clone(),
                seq,
                message,
            });
        }
        let cancelled = cancel.is_cancelled();
        session.emit(&server.hub, |seq| ServerMessage::TurnDone {
            session: id.clone(),
            seq,
            cancelled,
        });
        session.update_info(&server.hub, |info| info.state = TurnState::Idle);
    }
}

fn accept_user_message(server: &Arc<Server>, session: &Arc<Session>, message: ChatMessage) {
    let mut content = message.content.clone();
    if !message.pages.is_empty() {
        content.push_str("\n\nAttached pages:\n");
        for page in &message.pages {
            content.push_str(&format!("- {page}\n"));
        }
    }
    if let Some(MessageSource::App { site }) = &message.source {
        content = format!("[from the {site} app]\n{content}");
    }
    let stored = session.push_history(ModelMessage::User {
        content,
        images: message.images.clone(),
    });
    if let Err(e) = stored {
        tracing::error!("session {}: {e}", session.id());
    }
    let preview = message.content.chars().take(80).collect::<String>();
    session.emit(&server.hub, |seq| ServerMessage::MessageDone {
        session: session.id().to_string(),
        seq,
        message,
    });
    session.update_info(&server.hub, |info| info.preview = Some(preview));
}

/// Model call → tools → model call ... until the model answers without tool calls.
async fn run_turn(
    server: &Arc<Server>,
    session: &Arc<Session>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let id = session.id().to_string();
    let agent = Arc::new(
        AgentDefinition::load(&server.workspace, &session.info().agent)
            .map_err(|e| e.to_string())?,
    );
    let tools = server.tools.specs_for(&agent);
    let context = ToolContext {
        server: server.clone(),
        session: session.clone(),
        agent: agent.clone(),
        cancel: cancel.clone(),
    };

    for _ in 0..server.config.agent.max_steps {
        if server.slots.is_full() {
            session.update_info(&server.hub, |info| info.state = TurnState::Waiting);
        }
        let slot = tokio::select! {
            slot = server.slots.acquire() => slot,
            _ = cancel.cancelled() => return Ok(()),
        };
        session.update_info(&server.hub, |info| info.state = TurnState::Running);

        // The prompt is rebuilt every step: memory or the active project may have changed.
        let mut messages = vec![ModelMessage::System {
            content: system_prompt(&agent, &server.workspace, &session.info()),
        }];
        messages.extend(prepare_history(server, session.history()));
        let mut stream = server
            .provider
            .stream(ModelRequest {
                messages,
                tools: tools.clone(),
            })
            .await?;

        let mut content = String::new();
        let mut thinking = String::new();
        let finished = loop {
            let event = tokio::select! {
                event = stream.recv() => event,
                _ = cancel.cancelled() => break None,
            };
            match event {
                Some(Ok(ModelEvent::Thinking(text))) => {
                    thinking.push_str(&text);
                    session.emit(&server.hub, |seq| ServerMessage::ThinkingDelta {
                        session: id.clone(),
                        seq,
                        text,
                    });
                }
                Some(Ok(ModelEvent::Content(text))) => {
                    content.push_str(&text);
                    session.emit(&server.hub, |seq| ServerMessage::MessageDelta {
                        session: id.clone(),
                        seq,
                        text,
                    });
                }
                Some(Ok(ModelEvent::Finished {
                    tool_calls, usage, ..
                })) => break Some((tool_calls, usage)),
                Some(Err(e)) => return Err(e),
                None => return Err("model stream ended unexpectedly".into()),
            }
        };
        drop(stream);
        drop(slot);

        let (tool_calls, usage) = finished.unwrap_or_default();
        if let Some(usage) = usage {
            session.update_info(&server.hub, |info| {
                info.context_tokens = usage.prompt_tokens + usage.completion_tokens
            });
        }
        record_assistant(server, session, content, thinking, &tool_calls)?;
        if cancel.is_cancelled() || tool_calls.is_empty() {
            return Ok(());
        }

        for call in &tool_calls {
            let (result, is_error) = if cancel.is_cancelled() {
                ("cancelled by the user".to_string(), true)
            } else {
                session.emit(&server.hub, |seq| ServerMessage::ToolCallStarted {
                    session: id.clone(),
                    seq,
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    args: parse_args(&call.arguments),
                });
                server.tools.call(&context, call).await
            };
            session.emit(&server.hub, |seq| ServerMessage::ToolResult {
                session: id.clone(),
                seq,
                call_id: call.id.clone(),
                result: result.clone(),
                is_error,
            });
            session
                .push_history(ModelMessage::Tool {
                    call_id: call.id.clone(),
                    content: result,
                })
                .map_err(|e| e.to_string())?;
        }
    }
    Err(format!(
        "reached the limit of {} model calls in one turn",
        server.config.agent.max_steps
    ))
}

fn record_assistant(
    server: &Arc<Server>,
    session: &Arc<Session>,
    content: String,
    thinking: String,
    tool_calls: &[ModelToolCall],
) -> Result<(), String> {
    if content.is_empty() && tool_calls.is_empty() && thinking.is_empty() {
        return Ok(());
    }
    let thinking = (!thinking.is_empty()).then_some(thinking);
    session
        .push_history(ModelMessage::Assistant {
            content: content.clone(),
            thinking: thinking.clone(),
            tool_calls: tool_calls.to_vec(),
        })
        .map_err(|e| e.to_string())?;
    let message = ChatMessage {
        role: Role::Assistant,
        content,
        thinking,
        tool_calls: tool_calls
            .iter()
            .map(|c| ToolCall {
                call_id: c.id.clone(),
                name: c.name.clone(),
                args: parse_args(&c.arguments),
            })
            .collect(),
        pages: vec![],
        images: vec![],
        source: None,
    };
    session.emit(&server.hub, |seq| ServerMessage::MessageDone {
        session: session.id().to_string(),
        seq,
        message,
    });
    Ok(())
}

/// Drops thinking from earlier turns (only the current turn keeps it) and
/// replaces image site URLs with data URLs.
fn prepare_history(server: &Server, mut history: Vec<ModelMessage>) -> Vec<ModelMessage> {
    let current_turn = history
        .iter()
        .rposition(|m| matches!(m, ModelMessage::User { .. }))
        .unwrap_or(0);
    for (i, message) in history.iter_mut().enumerate() {
        match message {
            ModelMessage::Assistant { thinking, .. } if i < current_turn => *thinking = None,
            ModelMessage::User { images, .. } => {
                *images = images
                    .iter()
                    .filter_map(|url| data_url(server, url))
                    .collect();
            }
            _ => {}
        }
    }
    history
}

fn data_url(server: &Server, url: &str) -> Option<String> {
    let path = server.workspace.locate(&SiteUrl::parse(url).ok()?).ok()?;
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    };
    let bytes = std::fs::read(&path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn parse_args(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::Value::String(arguments.to_string()))
}

fn system_message(content: String) -> ChatMessage {
    ChatMessage {
        role: Role::System,
        content,
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        source: Some(MessageSource::Server),
    }
}
