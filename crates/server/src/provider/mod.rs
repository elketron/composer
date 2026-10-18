//! Talking to the model: an OpenAI-compatible chat completions API with streaming.

mod openai_provider;
mod partial_tool_calls;
mod sse_decoder;
mod types;

use async_trait::async_trait;

pub use openai_provider::OpenAiProvider;
pub use partial_tool_calls::PartialToolCalls;
pub use sse_decoder::SseDecoder;
pub use types::{
    ModelEvent, ModelMessage, ModelRequest, ModelStream, ModelToolCall, ToolSpec, Usage,
};

#[async_trait]
pub trait Provider: Send + Sync {
    /// The model id requests go to.
    fn model(&self) -> &str;

    /// Starts a streaming completion. Dropping the returned receiver cancels it.
    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, String>;
}
