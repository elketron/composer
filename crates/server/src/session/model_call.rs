//! One model call (holding a slot while it streams) and one tool call, as used by
//! turns and by compaction.

use std::sync::Arc;

use base64::Engine;
use composer_core::SiteUrl;
use composer_protocol::{ServerMessage, TurnState};
use tokio_util::sync::CancellationToken;

use crate::attachments::{ATTACHMENT_SCHEME, attachment_data_url};
use crate::provider::{ModelEvent, ModelMessage, ModelRequest, ModelToolCall};
use crate::server::Server;
use crate::session::{Session, StepOutput};
use crate::tools::{CallResult, ToolContext, text_result};

/// Streams one completion. Holds a model slot only for the call itself.
/// With `live`, thinking and text are streamed to clients as deltas.
pub async fn model_step(
    server: &Arc<Server>,
    session: &Arc<Session>,
    mut request: ModelRequest,
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
    request.model = request.model.or(session.info().model);
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

/// Runs a tool call, showing it in the chat. Returns its result.
pub async fn tool_step(
    server: &Arc<Server>,
    context: &ToolContext,
    call: &ModelToolCall,
) -> CallResult {
    let session = &context.session;
    let id = session.id().to_string();
    let result = if context.cancel.is_cancelled() {
        text_result("cancelled by the user".to_string(), true)
    } else {
        session.emit(&server.hub, |seq| ServerMessage::ToolCallStarted {
            session: id.clone(),
            seq,
            call_id: call.id.clone(),
            name: call.name.clone(),
            args: parse_args(&call.arguments),
            parent: None,
        });
        server.tools.call(context, call).await
    };
    session.emit(&server.hub, |seq| ServerMessage::ToolResult {
        session: id.clone(),
        seq,
        call_id: call.id.clone(),
        result: result.text.clone(),
        is_error: result.is_error,
    });
    result
}

/// A tool's images (data URLs), stored with the session's attachments like an upload (shrunk,
/// EXIF dropped): history keeps their references, not the bytes. One that can't be stored is
/// left out.
pub fn store_images(
    server: &Server,
    session: &Session,
    tool: &str,
    images: &[String],
) -> Vec<String> {
    let config = server.config();
    images
        .iter()
        .enumerate()
        .filter_map(|(i, url)| {
            let (mime, data) = url.strip_prefix("data:")?.split_once(";base64,")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .ok()?;
            let ext = mime.strip_prefix("image/").unwrap_or("png");
            let name = format!("{}-{}.{ext}", tool.replace("__", "-"), i + 1);
            match crate::attachments::save(
                &server.home,
                &config.attachments,
                session.id(),
                &name,
                &bytes,
            ) {
                Ok(saved) => Some(format!(
                    "{ATTACHMENT_SCHEME}{}/{}",
                    session.id(),
                    saved.file
                )),
                Err(e) => {
                    tracing::warn!("an image from {tool}: {e}");
                    None
                }
            }
        })
        .collect()
}

/// Drops thinking from earlier turns and from the current turn's older segments, and replaces
/// image site URLs with data URLs. `thinking_budget`: the thinking tokens a segment holds.
pub fn prepare_history(
    server: &Server,
    mut history: Vec<ModelMessage>,
    thinking_budget: u32,
) -> Vec<ModelMessage> {
    let turn = history
        .iter()
        .rposition(|m| matches!(m, ModelMessage::User { .. }))
        .unwrap_or(0);
    let current_turn = thinking_kept_from(&history, turn, thinking_budget);
    let tool_images = server.config().provider.tool_images;
    for (i, message) in history.iter_mut().enumerate() {
        match message {
            ModelMessage::Assistant { thinking, .. } if i < current_turn => *thinking = None,
            // A server that takes no images in tool results: the text names them.
            ModelMessage::Tool { images, .. } if !tool_images => images.clear(),
            ModelMessage::User { images, .. } | ModelMessage::Tool { images, .. } => {
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

/// A segment's thinking: an eighth of the context at which a session is compacted.
pub fn thinking_budget(server: &Server) -> u32 {
    server.config().agent.compaction_threshold / 8
}

/// Where the current turn's thinking starts being sent: a worker does its whole ticket in one
/// turn, and resending every step's reasoning filled its context with old plans. Dropping it
/// step by step rewrote the prompt a few steps back on every call, so a server's prompt cache
/// (which resumes only an exact prefix) read it all again each time. So the turn's steps go in
/// segments of about `budget` thinking tokens, cut from the turn's start: adding steps never
/// moves a cut, the prompt only grows, and it changes before its end once a segment fills.
/// Thinking before the last cut is dropped.
fn thinking_kept_from(history: &[ModelMessage], turn: usize, budget: u32) -> usize {
    let mut cut = turn;
    let mut held = 0u32;
    for (i, message) in history.iter().enumerate().skip(turn) {
        if let ModelMessage::Assistant {
            thinking: Some(thinking),
            ..
        } = message
        {
            // The step that fills a segment starts the next one: the latest keeps its thinking.
            let tokens = (thinking.len() / 4) as u32;
            if held > 0 && held + tokens > budget {
                cut = i;
                held = 0;
            }
            held += tokens;
        }
    }
    cut
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
    if url.starts_with(ATTACHMENT_SCHEME) {
        return attachment_data_url(&server.home, url);
    }
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
