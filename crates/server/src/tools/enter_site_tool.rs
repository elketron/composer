use async_trait::async_trait;
use serde_json::{Value, json};

use crate::agent::enter_site_as_agent;
use crate::tools::{Tool, ToolContext};

pub struct EnterSiteTool;

#[async_trait]
impl Tool for EnterSiteTool {
    fn name(&self) -> &'static str {
        "enter_site"
    }

    fn description(&self) -> &'static str {
        "Enter a site: from your next turn its index.md is part of your context and its `tools:` \
         join your base tools. `leave_site` returns to the base set."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "site": { "type": "string", "description": "a site name, e.g. orchestration" }
            },
            "required": ["site"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let site = args["site"].as_str().unwrap_or_default();
        let entered =
            enter_site_as_agent(&context.server, &context.session, &context.agent, site).await?;
        let mut notes = entered.notes();
        if entered.live_view {
            notes.push_str(" Its live view is a tab on the user's stage.");
        }
        Ok(format!(
            "entered site://{} ({}). Its index, tools and MCP servers apply from your next turn; \
             `leave_site` returns. {notes}",
            entered.site, entered.title
        )
        .trim_end()
        .to_string())
    }
}
