//! One model call (holding a slot while it streams) and one tool call, as used by
//! turns and by compaction.

use std::sync::Arc;

use base64::Engine;
use composer_core::SiteUrl;
use composer_protocol::{ServerMessage, TurnState};
use tokio_util::sync::CancellationToken;

use crate::provider::{ModelEvent, ModelMessage, ModelRequest, ModelToolCall};
use crate::server::Server;
use crate::session::{Session, StepOutput};
use crate::tools::ToolContext;

/// Streams one completion. Holds a model slot only for the call itself.
/// With `live`, thinking and text are streamed to clients as deltas.
pub async fn model_step(
    server: &Arc<Server>,
    session: &Arc<Session>,
    request: ModelRequest,
    cancel: &CancellationToken,
    live: bool,
) -> Result<StepOutput, String> {
    if server.slots.is_full() {
        session.update_info(&server.hub, |info| info.state = TurnState::Waiting);
    }
    let slot = tokio::select! {
        slot = server.slots.acquire() => slot?,
        _ = cancel.cancelled() => return Ok(StepOutput { cancelled: true, ..StepOutput::default() }),
    };
    session.update_info(&server.hub, |info| info.state = TurnState::Running);

    let id = session.id().to_string();
    let mut stream = server.provider.stream(request).await?;
    let mut out = StepOutput::default();
    loop {
        let event = tokio::select! {
            event = stream.recv() => event,
            _ = cancel.cancelled() => {
                out.cancelled = true;
                break;
            }
        };
        match event {
            Some(Ok(ModelEvent::Thinking(text))) => {
                out.thinking.push_str(&text);
                if live {
                    session.emit(&server.hub, |seq| ServerMessage::ThinkingDelta {
                        session: id.clone(),
                        seq,
                        text,
                    });
                }
            }
            Some(Ok(ModelEvent::Content(text))) => {
                out.content.push_str(&text);
                if live {
                    session.emit(&server.hub, |seq| ServerMessage::MessageDelta {
                        session: id.clone(),
                        seq,
                        text,
                    });
                }
            }
            Some(Ok(ModelEvent::Finished {
                tool_calls, usage, ..
            })) => {
                out.tool_calls = tool_calls;
                out.usage = usage;
                break;
            }
            Some(Err(e)) => return Err(e),
            None => return Err("model stream ended unexpectedly".into()),
        }
    }
    drop(stream);
    drop(slot);
    Ok(out)
}

/// Runs a tool call, showing it in the chat. Returns the result and whether it failed.
pub async fn tool_step(
    server: &Arc<Server>,
    context: &ToolContext,
    call: &ModelToolCall,
) -> (String, bool) {
    let session = &context.session;
    let id = session.id().to_string();
    let (result, is_error) = if context.cancel.is_cancelled() {
        ("cancelled by the user".to_string(), true)
    } else {
        session.emit(&server.hub, |seq| ServerMessage::ToolCallStarted {
            session: id.clone(),
            seq,
            call_id: call.id.clone(),
            name: call.name.clone(),
            args: parse_args(&call.arguments),
        });
        server.tools.call(context, call).await
    };
    session.emit(&server.hub, |seq| ServerMessage::ToolResult {
        session: id.clone(),
        seq,
        call_id: call.id.clone(),
        result: result.clone(),
        is_error,
    });
    (result, is_error)
}

/// Drops thinking from earlier turns (only the current turn keeps it) and
/// replaces image site URLs with data URLs.
pub fn prepare_history(server: &Server, mut history: Vec<ModelMessage>) -> Vec<ModelMessage> {
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

pub fn parse_args(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::Value::String(arguments.to_string()))
}

/// Rough token count (about 4 characters per token) for when the provider hasn't told us yet.
pub fn estimate_tokens(messages: &[ModelMessage]) -> u32 {
    let chars: usize = messages
        .iter()
        .map(|m| match m {
            ModelMessage::System { content }
            | ModelMessage::User { content, .. }
            | ModelMessage::Tool { content, .. } => content.len(),
            ModelMessage::Assistant {
                content,
                tool_calls,
                ..
            } => content.len() + tool_calls.iter().map(|c| c.arguments.len()).sum::<usize>(),
        })
        .sum();
    (chars / 4) as u32
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
