//! Talking to the model: an OpenAI-compatible chat completions API with streaming.

mod openai_provider;
mod partial_tool_calls;
mod sse_decoder;
mod types;

use async_trait::async_trait;
use composer_protocol::ModelInfo;

pub use openai_provider::OpenAiProvider;
pub use partial_tool_calls::PartialToolCalls;
pub use sse_decoder::SseDecoder;
pub use types::{
    ModelEvent, ModelMessage, ModelRequest, ModelStream, ModelToolCall, ToolSpec, Usage,
};

/// Why a completion could not start.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// The provider, or a gateway in front of it, couldn't answer just now (502, 503, 504, the
    /// connection failed): the same call later may work.
    Unavailable(String),
    /// It answered no (a bad request, a missing model, an auth error): the same call fails again.
    Refused(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(error) | Self::Refused(error) => f.write_str(error),
        }
    }
}

impl From<CallError> for String {
    fn from(error: CallError) -> Self {
        error.to_string()
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// The model id requests go to.
    fn model(&self) -> &str;

    /// Starts a streaming completion. Dropping the returned receiver cancels it.
    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, CallError>;

    /// The models it serves, for picking one per session. None listed: only its own.
    async fn models(&self) -> Result<Vec<ModelInfo>, String> {
        Ok(vec![])
    }
}
