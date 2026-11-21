use async_trait::async_trait;
use serde_json::{Value, json};

use crate::agent::{SiteSet, set_site};
use crate::tools::{Tool, ToolContext};

pub struct SetSiteTool;

#[async_trait]
impl Tool for SetSiteTool {
    fn name(&self) -> &'static str {
        "set_site"
    }

    fn description(&self) -> &'static str {
        "Set the site you work in: from your next turn its index.md is part of your context, its \
         `tools:` and MCP servers join your tools, and its `folder:` is where terminals start. \
         `site: null` leaves it."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "site": {
                    "type": ["string", "null"],
                    "description": "a site name, e.g. plans; null to leave the site"
                }
            }
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let site = args["site"].as_str();
        let set = set_site(&context.server, &context.session, site).await?;
        Ok(match set {
            SiteSet::Entered(_) => format!("{} It applies from your next turn.", set.text()),
            _ => set.text(),
        })
    }
}
