//! The agent's tools: browsing and editing the web of pages.

mod actions_tool;
mod answer_approval_tool;
mod args;
mod continue_worker_tool;
mod edit_tool;
mod finish_tool;
mod list_sites_tool;
mod markdown_sections;
mod merge_ticket_tool;
mod new_app_tool;
mod new_pipeline_tool;
mod new_project_tool;
mod new_site_tool;
mod new_work_tool;
mod open_tool;
mod run_stage_tool;
mod run_tool;
pub(crate) mod scaffold;
mod search_tool;
mod set_project_tool;
mod spawn_worker_tool;
mod stop_worker_tool;
mod tool_context;
mod tool_registry;
mod write_tool;

use async_trait::async_trait;
use serde_json::Value;

pub(crate) use markdown_sections::append_list_item;
pub(crate) use new_site_tool::add_to_home;
pub(crate) use set_project_tool::{expand_home, resolve_project, tilde};
pub use tool_context::ToolContext;
pub use tool_registry::ToolRegistry;

/// What a tool call gave back: its text, whether it failed, and the images in it (data URLs:
/// an MCP server's screenshot).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallResult {
    pub text: String,
    pub is_error: bool,
    pub images: Vec<String>,
}

/// A result that is only text.
pub fn text_result(text: String, is_error: bool) -> CallResult {
    CallResult {
        text,
        is_error,
        images: vec![],
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    /// Shown to the model; say when to use the tool and what it returns.
    fn description(&self) -> &'static str;
    /// JSON schema of the arguments object.
    fn parameters(&self) -> Value;
    /// `Err` is reported to the model as a failed call, so it can recover.
    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String>;

    /// The whole result; a tool that gives back images (`open` on a picture) has its own.
    async fn call_result(&self, context: &ToolContext, args: Value) -> CallResult {
        match self.call(context, args).await {
            Ok(text) => text_result(text, false),
            Err(error) => text_result(error, true),
        }
    }
}
