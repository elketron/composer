use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// One message of the conversation as the model sees it.
/// Also the format of a session's `history.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ModelMessage {
    System {
        content: String,
    },
    User {
        content: String,
        /// Site URLs in history; replaced by data URLs when sent.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<String>,
    },
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thinking: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ModelToolCall>,
    },
    Tool {
        call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON as produced by the model; may be invalid.
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON schema of the arguments object.
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelEvent {
    Thinking(String),
    Content(String),
    /// Always the last event of a successful stream.
    Finished {
        tool_calls: Vec<ModelToolCall>,
        usage: Option<Usage>,
        finish_reason: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

pub type ModelStream = mpsc::Receiver<Result<ModelEvent, String>>;
