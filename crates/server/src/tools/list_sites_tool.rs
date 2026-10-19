use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::{Tool, ToolContext};

pub struct ListSitesTool;

#[async_trait]
impl Tool for ListSitesTool {
    fn name(&self) -> &'static str {
        "list_sites"
    }

    fn description(&self) -> &'static str {
        "List all sites with their titles, and whether they have actions (a justfile) or an app."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, context: &ToolContext, _args: Value) -> Result<String, String> {
        let sites = context
            .server
            .workspace
            .sites()
            .map_err(|e| e.to_string())?;
        let lines: Vec<String> = sites
            .iter()
            .filter(|s| context.agent.sites.can_read(&s.name))
            .map(|s| {
                let mut line = format!("- site://{} — {}", s.name, s.title);
                if s.has_justfile {
                    line.push_str(" [actions]");
                }
                if s.app_built {
                    line.push_str(" [app]");
                } else if s.has_ui {
                    line.push_str(" [app, not built: run its ui-build action]");
                }
                if !s.has_index {
                    line.push_str(" [no index.md]");
                }
                line
            })
            .collect();
        Ok(lines.join("\n"))
    }
}
