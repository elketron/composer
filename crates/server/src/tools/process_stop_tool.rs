use async_trait::async_trait;
use serde_json::{Value, json};

use crate::terminals::StoppedBy;
use crate::tools::terminal_tools::terminal;
use crate::tools::{Tool, ToolContext};

/// Stops a background process of the session, and what it started.
pub struct ProcessStopTool;

#[async_trait]
impl Tool for ProcessStopTool {
    fn name(&self) -> &'static str {
        "process_stop"
    }

    fn description(&self) -> &'static str {
        "Stop a background process you started with `process_start` (by its terminal id), and \
         everything it started. `process_list` lists them."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "its terminal id" }
            },
            "required": ["id"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let terminal = terminal(context, &args)?;
        context
            .server
            .terminals
            .stop_process(&terminal.id, StoppedBy::Agent)?;
        let command = terminal
            .background
            .as_ref()
            .map(|b| b.command.as_str())
            .unwrap_or_default();
        Ok(format!(
            "stopped background process {} (`{command}`)",
            terminal.id
        ))
    }
}
