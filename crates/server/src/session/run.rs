//! `run(kind)`: a single shot, a workflow run or an agentic run, writing to a [`RunSink`].

use std::sync::Arc;

use composer_protocol::{ChatMessage, MessageSource, Role, RunKind, ServerMessage, ToolCall};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agent::{RunSettings, Site, single_shot_prompt, system_prompt};
use crate::provider::{ModelMessage, ModelRequest, ModelToolCall};
use crate::server::Server;
use crate::session::compaction::compact;
use crate::session::model_call::{
    model_step_continuing, parse_args, prepare_history, store_images, thinking_budget, tool_step,
};
use crate::session::turn_budget::TurnBudget;
use crate::session::{MemorySink, RunSink};
use crate::tools::ToolContext;

/// How a run that used up its tokens (or steps) ends: "Turn stopped: reached the turn limit (…)".
pub const TURN_LIMIT: &str = "reached the turn limit";

/// The tool a workflow run ends with.
const FINISH: &str = "finish";

/// Runs `settings.kind` on what `sink` holds (its history ends with the input), and gives the
/// result to the sink as well as back:
/// - a single shot is one model call without tools; the answer is the result;
/// - a workflow run loops with the read-only tools until the model calls `finish(value)` (the
///   value is the result) or stops (its answer is);
/// - an agentic run loops with every tool until the model stops; its answer is the result.
///
/// Before each call of a stored run, a context over the threshold is compacted first.
pub async fn run(
    server: &Arc<Server>,
    settings: &RunSettings,
    sink: Arc<dyn RunSink>,
    cancel: &CancellationToken,
) -> Result<Value, String> {
    let site = sink
        .site()
        .as_deref()
        .and_then(|name| Site::load(&server.web, name).ok())
        .map(Arc::new);
    let tools = server
        .tools
        .specs_with_mcp(server, settings.kind, site.as_deref())
        .await;
    let context = ToolContext {
        server: server.clone(),
        run: sink.clone(),
        kind: settings.kind,
        only: None,
        site,
        cancel: cancel.clone(),
    };

    let (turn_tokens, max_steps) = {
        let config = server.config();
        (config.agent.turn_tokens, config.agent.max_steps)
    };
    let mut budget = TurnBudget::new(turn_tokens);
    let mut steps = 0;
    loop {
        if budget.exceeded() {
            return Err(format!(
                "{TURN_LIMIT} ({} of {turn_tokens} tokens in one turn)",
                budget.used()
            ));
        }
        if max_steps > 0 && steps >= max_steps {
            return Err(format!(
                "{TURN_LIMIT} ({max_steps} model calls in one turn)"
            ));
        }
        steps += 1;
        if let Some(session) = sink.session()
            && session.info().context_tokens >= server.config().agent.compaction_threshold
        {
            compact(server, &session, cancel, true).await?;
        }
        // The prompt is rebuilt every step: memory or the site may have changed. A single shot's
        // is only its folder's instructions.
        let system = match settings.kind {
            RunKind::SingleShot => single_shot_prompt(
                &server.web,
                sink.site().as_deref(),
                settings.folder.as_deref(),
            ),
            _ => Some(system_prompt(
                settings,
                &server.web,
                &server.index.read().expect("index lock"),
                sink.site().as_deref(),
                sink.id(),
            )),
        };
        let mut messages: Vec<ModelMessage> = system
            .map(|content| ModelMessage::System { content })
            .into_iter()
            .collect();
        messages.extend(prepare_history(
            server,
            sink.history(),
            thinking_budget(server),
        ));
        let request = ModelRequest {
            messages,
            tools: tools.clone(),
            model: settings.model.clone(),
            reasoning_effort: settings.reasoning.clone(),
        };
        let step = model_step_continuing(server, &*sink, request, cancel, true).await?;
        let written: String = step
            .tool_calls
            .iter()
            .map(|c| c.arguments.as_str())
            .chain([step.content.as_str(), step.thinking.as_str()])
            .collect();
        budget.model_call(step.usage.as_ref(), &written);
        if let Some(usage) = step.usage {
            sink.set_context_tokens(usage.prompt_tokens + usage.completion_tokens);
        }
        let answer = step.content.clone();
        record_assistant(&*sink, step.content, step.thinking, &step.tool_calls)?;
        let stopped = step.cancelled || cancel.is_cancelled() || step.tool_calls.is_empty();
        if stopped || settings.kind == RunKind::SingleShot {
            return Ok(finished(&*sink, Value::String(answer)));
        }
        let mut value = None;
        for call in &step.tool_calls {
            let result = tool_step(server, &context, call).await;
            budget.tool_results(&result.text);
            if call.name == FINISH && !result.is_error {
                value = Some(parse_args(&call.arguments)["value"].clone());
            }
            let images = match sink.session() {
                Some(session) => store_images(server, &session, &call.name, &result.images),
                // Not stored: the images stay data URLs in the run's own history.
                None => result.images,
            };
            sink.push_history(ModelMessage::Tool {
                call_id: call.id.clone(),
                content: result.text,
                images,
            })?;
            // Something only the user could allow, in a run nobody can be asked in.
            if let Some(error) = sink.aborted() {
                return Err(error);
            }
        }
        if let Some(value) = value {
            return Ok(finished(&*sink, value));
        }
    }
}

/// A run of `settings` on `input` that is not stored: no session, no folder. Its result, and the
/// sink with what it did.
pub async fn run_once(
    server: &Arc<Server>,
    settings: &RunSettings,
    input: &str,
    site: Option<String>,
    cancel: &CancellationToken,
) -> (Result<Value, String>, Arc<MemorySink>) {
    let sink = Arc::new(MemorySink::new(input, site));
    let result = run(server, settings, sink.clone(), cancel).await;
    (result, sink)
}

fn finished(sink: &dyn RunSink, value: Value) -> Value {
    sink.finish(&value);
    value
}

fn record_assistant(
    sink: &dyn RunSink,
    content: String,
    thinking: String,
    tool_calls: &[ModelToolCall],
) -> Result<(), String> {
    if content.is_empty() && tool_calls.is_empty() && thinking.is_empty() {
        return Ok(());
    }
    let thinking = (!thinking.is_empty()).then_some(thinking);
    sink.push_history(ModelMessage::Assistant {
        content: content.clone(),
        thinking: thinking.clone(),
        tool_calls: tool_calls.to_vec(),
    })?;
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
        attachments: vec![],
        source: None,
    };
    let id = sink.id().to_string();
    sink.emit(Box::new(move |seq| ServerMessage::MessageDone {
        session: id,
        seq,
        message,
    }));
    Ok(())
}

pub(crate) fn system_message(content: String) -> ChatMessage {
    ChatMessage {
        role: Role::System,
        content,
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        attachments: vec![],
        source: Some(MessageSource::Server),
    }
}
