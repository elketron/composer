use async_trait::async_trait;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::tools::args::EditArgs;
use crate::tools::{Tool, ToolContext};

pub struct EditTool;

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Change part of an existing page: replaces `old_text` with `new_text`. \
         `old_text` must match the page exactly (whitespace included) and occur once, \
         unless `replace_all` is true. Open the page first to copy the exact text."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "old_text": { "type": "string" },
                "new_text": { "type": "string" },
                "replace_all": { "type": "boolean" }
            },
            "required": ["url", "old_text", "new_text"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: EditArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let url = context.url(&args.url)?;
        context.check_write(&url)?;
        if args.old_text.is_empty() {
            return Err("old_text is empty; use `write` to replace a whole page".into());
        }
        let page = context
            .server
            .workspace
            .read_page(&url)
            .map_err(|e| e.to_string())?;
        let count = page.raw.matches(&args.old_text).count();
        let updated = match count {
            0 => {
                return Err(format!(
                    "old_text not found in {}; open the page and copy the exact text",
                    page.url
                ));
            }
            1 => page.raw.replacen(&args.old_text, &args.new_text, 1),
            _ if args.replace_all => page.raw.replace(&args.old_text, &args.new_text),
            n => {
                return Err(format!(
                    "old_text occurs {n} times; include more context or set replace_all"
                ));
            }
        };
        context
            .server
            .write_page(&page.url, &updated, ChangedBy::Agent)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "edited {} ({} replacement{})",
            page.url,
            count,
            if count == 1 { "" } else { "s" }
        ))
    }
}
