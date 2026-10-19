use async_trait::async_trait;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::reminders::claim_reminder;
use crate::tools::args::WriteArgs;
use crate::tools::{Tool, ToolContext};

pub struct WriteTool;

/// Said when a reminder page is claimed for the writing session.
pub(crate) const CLAIMED: &str =
    " (a reminder: `session:` set to this session, where it will be posted when due)";

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &'static str {
        "write"
    }

    fn description(&self) -> &'static str {
        "Create a page, or replace a page's entire content. For changes to an existing page use `edit`. \
         Include frontmatter (title, type, ...) as the site's index.md describes. \
         `site://memory/topics/rust` creates topics/rust.md."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "content": { "type": "string", "description": "the full file content" }
            },
            "required": ["url", "content"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: WriteArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let url = context.url(&args.url)?;
        context.check_write(&url)?;
        let claimed = claim_reminder(&args.content, context.session.id());
        let content = claimed.as_deref().unwrap_or(&args.content);
        let (written, created) = context
            .server
            .write_page(&url, content, ChangedBy::Agent)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "{} {written}{}",
            if created { "created" } else { "replaced" },
            if claimed.is_some() { CLAIMED } else { "" }
        ))
    }
}
