use std::collections::HashSet;
use std::sync::Arc;

use agent_client_protocol::v1::schema::{
    ContentBlock, SessionUpdate, ToolCallContent, ToolCallStatus,
};
use composer_protocol::{ChatMessage, Role, ServerMessage, ToolCall};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::provider::ModelMessage;
use crate::session::RunSink;

/// One turn of an outside agent, as composer's own events: its `session/update`s become the
/// deltas, messages and tool calls every client shows, in the session's transcript.
///
/// The agent's text comes in segments split by its tool calls. A segment ends in a
/// `message.done` (carrying the tool call that ended it, as composer's own loop records a step);
/// the last segment's text is the turn's answer.
pub struct AcpTurn {
    sink: Arc<dyn RunSink>,
    /// The turn's own: a permission request asked during it closes when it is stopped.
    pub cancel: CancellationToken,
    text: String,
    thinking: String,
    /// Tool calls shown as started, and those that have their result.
    started: HashSet<String>,
    finished: HashSet<String>,
    /// The last segment with text.
    answer: String,
}

impl AcpTurn {
    pub fn new(sink: Arc<dyn RunSink>, cancel: CancellationToken) -> Self {
        Self {
            sink,
            cancel,
            text: String::new(),
            thinking: String::new(),
            started: HashSet::new(),
            finished: HashSet::new(),
            answer: String::new(),
        }
    }

    /// One `session/update` of the agent's.
    pub fn update(&mut self, update: SessionUpdate) {
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                let text = block_text(&chunk.content);
                self.text.push_str(&text);
                let id = self.sink.id().to_string();
                self.sink
                    .emit(Box::new(move |seq| ServerMessage::MessageDelta {
                        session: id,
                        seq,
                        text,
                    }));
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                let text = block_text(&chunk.content);
                self.thinking.push_str(&text);
                let id = self.sink.id().to_string();
                self.sink
                    .emit(Box::new(move |seq| ServerMessage::ThinkingDelta {
                        session: id,
                        seq,
                        text,
                    }));
            }
            SessionUpdate::ToolCall(call) => {
                let id = call.tool_call_id.0.to_string();
                let args = call.raw_input.clone().unwrap_or(Value::Null);
                self.start_tool(&id, &call.title, args);
                self.tool_status(&id, call.status, &call.content, call.raw_output.as_ref());
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let id = update.tool_call_id.0.to_string();
                let fields = update.fields;
                if !self.started.contains(&id) {
                    let title = fields.title.clone().unwrap_or_else(|| "tool".into());
                    let args = fields.raw_input.clone().unwrap_or(Value::Null);
                    self.start_tool(&id, &title, args);
                }
                if let Some(status) = fields.status {
                    let content = fields.content.unwrap_or_default();
                    self.tool_status(&id, status, &content, fields.raw_output.as_ref());
                }
            }
            // Plans, modes, commands, usage: nothing composer's clients show.
            _ => {}
        }
    }

    /// The turn ended: the last segment is recorded; the answer is the last text the agent
    /// wrote.
    pub fn finish(mut self) -> String {
        self.end_segment(None);
        if let Err(e) = self.sink.push_history(ModelMessage::Assistant {
            content: self.answer.clone(),
            thinking: None,
            tool_calls: vec![],
        }) {
            tracing::warn!("run {}: {e}", self.sink.id());
        }
        self.answer
    }

    fn start_tool(&mut self, id: &str, title: &str, args: Value) {
        if !self.started.insert(id.to_string()) {
            return;
        }
        self.end_segment(Some(ToolCall {
            call_id: id.to_string(),
            name: title.to_string(),
            args: args.clone(),
        }));
        let (session, call_id, name) = (self.sink.id().to_string(), id.to_string(), title.into());
        self.sink
            .emit(Box::new(move |seq| ServerMessage::ToolCallStarted {
                session,
                seq,
                call_id,
                name,
                args,
                parent: None,
            }));
    }

    fn tool_status(
        &mut self,
        id: &str,
        status: ToolCallStatus,
        content: &[ToolCallContent],
        raw_output: Option<&Value>,
    ) {
        let is_error = match status {
            ToolCallStatus::Completed => false,
            ToolCallStatus::Failed => true,
            _ => return,
        };
        if !self.finished.insert(id.to_string()) {
            return;
        }
        let mut result: Vec<String> = content.iter().map(tool_content_text).collect();
        if result.is_empty()
            && let Some(output) = raw_output
        {
            result.push(match output {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            });
        }
        let (session, call_id, result) = (self.sink.id().to_string(), id.to_string(), result);
        self.sink
            .emit(Box::new(move |seq| ServerMessage::ToolResult {
                session,
                seq,
                call_id,
                result: result.join("\n"),
                is_error,
            }));
    }

    /// Records the text and thinking so far as one assistant message, with the tool call that
    /// ended it.
    fn end_segment(&mut self, call: Option<ToolCall>) {
        let content = std::mem::take(&mut self.text);
        let thinking = std::mem::take(&mut self.thinking);
        if content.is_empty() && thinking.is_empty() && call.is_none() {
            return;
        }
        if !content.trim().is_empty() {
            self.answer = content.clone();
        }
        let message = ChatMessage {
            role: Role::Assistant,
            content,
            thinking: (!thinking.is_empty()).then_some(thinking),
            tool_calls: call.into_iter().collect(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
            source: None,
        };
        let id = self.sink.id().to_string();
        self.sink
            .emit(Box::new(move |seq| ServerMessage::MessageDone {
                session: id,
                seq,
                message,
            }));
    }
}

fn block_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text.clone(),
        ContentBlock::ResourceLink(link) => format!("[{}]({})", link.name, link.uri),
        ContentBlock::Image(_) => "[image]".into(),
        ContentBlock::Audio(_) => "[audio]".into(),
        _ => String::new(),
    }
}

fn tool_content_text(content: &ToolCallContent) -> String {
    match content {
        ToolCallContent::Content(content) => block_text(&content.content),
        ToolCallContent::Diff(diff) => format!("edited {}", diff.path.display()),
        ToolCallContent::Terminal(terminal) => format!("[terminal {}]", terminal.terminal_id.0),
        _ => String::new(),
    }
}
