use async_trait::async_trait;
use composer_core::Query;
use serde_json::{Value, json};

use crate::tools::args::SearchArgs;
use crate::tools::{Tool, ToolContext};

pub struct SearchTool;

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &'static str {
        "search"
    }

    fn description(&self) -> &'static str {
        "Find pages. Words match title and text; `key:value` matches frontmatter \
         (`type:ticket status:open`, `tags:rust`) and `site:memory` limits to a site. \
         Everything must match; quote phrases: \"device pairing\". Returns titles and URLs."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer", "description": "max results, default 20" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: SearchArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let query = Query::parse(&args.query);
        if query.is_empty() {
            return Err("empty query".into());
        }
        let limit = args.limit.unwrap_or(20).clamp(1, 100);
        let hits: Vec<_> = context
            .server
            .index
            .read()
            .expect("index lock")
            .search(&query)
            .into_iter()
            .filter(|h| context.agent.sites.can_read(h.url.site()))
            .collect();
        if hits.is_empty() {
            return Ok("no pages match".into());
        }
        let mut out: Vec<String> = hits
            .iter()
            .take(limit)
            .map(|h| format!("- {} — {}", h.title, h.url))
            .collect();
        if hits.len() > limit {
            out.push(format!("({} more; narrow the query)", hits.len() - limit));
        }
        Ok(out.join("\n"))
    }
}
