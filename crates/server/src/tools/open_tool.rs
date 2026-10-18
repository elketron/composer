use async_trait::async_trait;
use composer_protocol::ServerMessage;
use serde_json::{Value, json};

use crate::tools::args::OpenArgs;
use crate::tools::{Tool, ToolContext};

pub struct OpenTool;

#[async_trait]
impl Tool for OpenTool {
    fn name(&self) -> &'static str {
        "open"
    }

    fn description(&self) -> &'static str {
        "Open a page and read its full markdown, frontmatter included. \
         `site://memory` opens the site's index.md; `site://plans/x` finds x.md or x/index.md. \
         Relative paths are not resolved here: always pass a full site:// URL."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "e.g. site://memory/user" } },
            "required": ["url"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: OpenArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let url = context.url(&args.url)?;
        context.check_read(&url)?;
        let server = &context.server;
        let page = server
            .workspace
            .read_page(&url)
            .map_err(|e| format!("{e}. Use `search` to find pages, or `list_sites`."))?;

        let session = context.session.id().to_string();
        let canonical = page.url.to_string();
        context
            .session
            .emit(&server.hub, |seq| ServerMessage::AgentNavigated {
                session,
                seq,
                url: canonical,
            });

        let mut header = format!("url: {}", page.url);
        if let Some(error) = &page.frontmatter_error {
            header.push_str(&format!(
                "\nwarning: invalid frontmatter ({error}); fix it with `edit`"
            ));
        }
        let backlinks = server
            .index
            .read()
            .expect("index lock")
            .backlinks(&page.url);
        if !backlinks.is_empty() {
            let list: Vec<_> = backlinks.iter().map(ToString::to_string).collect();
            header.push_str(&format!("\nlinked from: {}", list.join(", ")));
        }
        Ok(format!("{header}\n\n{}", page.raw))
    }
}
