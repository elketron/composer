use async_trait::async_trait;
use serde_json::{Value, json};

use composer_protocol::DeskTab;

use crate::tools::stage_tools::{point, put, session_project};
use crate::tools::{Tool, ToolContext};

/// What is on the user's stage; the changes; pointing at a tab (ticket 0174).
pub struct DeskTool;

#[async_trait]
impl Tool for DeskTool {
    fn name(&self) -> &'static str {
        "desk"
    }

    fn description(&self) -> &'static str {
        "What is on the user's stage for this session (its tabs, the one shown, the ones the user \
         hasn't looked at yet). `changes: true` puts the session project's changes on it; `point` \
         (a tab id) asks the user's client to show that tab."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "changes": { "type": "boolean", "description": "put the project's changes on the stage" },
                "point": { "type": "string", "description": "a tab id to show, e.g. app:cad@game" }
            }
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        if args["changes"].as_bool().unwrap_or(false) {
            let project = session_project(context)
                .ok_or("this session has no project: set one first (`set_project`)")?;
            let tab = put(context, DeskTab::Changes { project })?;
            return Ok(format!("{tab} is on the stage"));
        }
        let desk = context.session.desk();
        if let Some(tab) = args["point"].as_str() {
            if !desk.tabs.iter().any(|t| t.id == tab) {
                return Err(format!(
                    "no tab `{tab}` on the desk (call `desk` to list them)"
                ));
            }
            point(context, desk, tab);
            return Ok(format!("pointed the user at {tab}"));
        }
        if desk.tabs.is_empty() {
            return Ok("The stage is empty (the project home).".into());
        }
        let lines: Vec<String> = desk
            .tabs
            .iter()
            .map(|t| {
                let shown = desk.active.as_deref() == Some(t.id.as_str());
                let mark = match (shown, t.unseen) {
                    (true, _) => " (shown)",
                    (false, true) => " (not looked at yet)",
                    _ => "",
                };
                format!("- {}{mark}", t.id)
            })
            .collect();
        Ok(lines.join("\n"))
    }
}
