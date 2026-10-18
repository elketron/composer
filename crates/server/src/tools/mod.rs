//! The agent's tools: browsing and editing the web of pages.

mod args;
mod edit_tool;
mod list_sites_tool;
mod open_tool;
mod search_tool;
mod set_project_tool;
mod tool_context;
mod tool_registry;
mod write_tool;

use async_trait::async_trait;
use serde_json::Value;

pub(crate) use set_project_tool::resolve_project;
pub use tool_context::ToolContext;
pub use tool_registry::ToolRegistry;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    /// Shown to the model; say when to use the tool and what it returns.
    fn description(&self) -> &'static str;
    /// JSON schema of the arguments object.
    fn parameters(&self) -> Value;
    /// `Err` is reported to the model as a failed call, so it can recover.
    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String>;
}
