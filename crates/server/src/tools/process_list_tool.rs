use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::{Tool, ToolContext};

/// The session's background processes.
pub struct ProcessListTool;

#[async_trait]
impl Tool for ProcessListTool {
    fn name(&self) -> &'static str {
        "process_list"
    }

    fn description(&self) -> &'static str {
        "List the background processes of this session that are running (`process_start`): \
         terminal id, command, folder, start time."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, context: &ToolContext, _args: Value) -> Result<String, String> {
        let session = context.session()?;
        let processes = context.server.terminals.processes(session.id());
        if processes.is_empty() {
            return Ok("no background processes running".into());
        }
        let lines: Vec<String> = processes
            .iter()
            .map(|p| {
                format!(
                    "- {}: `{}` in {} (since {})",
                    p.terminal, p.command, p.cwd, p.started
                )
            })
            .collect();
        Ok(lines.join("\n"))
    }
}
