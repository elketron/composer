use async_trait::async_trait;
use serde_json::{Value, json};

use composer_protocol::DeskTab;

use crate::tools::stage_tools::put;
use crate::tools::{Tool, ToolContext};

/// Puts a page on the user's stage (ticket 0174).
pub struct ShowPageTool;

#[async_trait]
impl Tool for ShowPageTool {
    fn name(&self) -> &'static str {
        "show_page"
    }

    fn description(&self) -> &'static str {
        "Put a page on the user's stage, next to the chat (a tab on this session's desk), e.g. \
         the plan you are talking about."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "site://plans/x" } },
            "required": ["url"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let raw = args["url"].as_str().unwrap_or_default();
        let url = context.url(raw)?;
        context.check_read(&url)?;
        context
            .server
            .web
            .locate(&url)
            .map_err(|e| format!("{e}. Use `search` to find pages."))?;
        let tab = put(
            context,
            DeskTab::Page {
                url: url.to_string(),
            },
        )?;
        Ok(format!("{tab} is on the stage"))
    }
}
