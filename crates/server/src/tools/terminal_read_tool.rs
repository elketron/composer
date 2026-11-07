use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::terminal_tools::{LINES, MAX_LINES, terminal};
use crate::tools::{Tool, ToolContext};

/// Reads a terminal of the session (ticket 0176).
pub struct TerminalReadTool;

#[async_trait]
impl Tool for TerminalReadTool {
    fn name(&self) -> &'static str {
        "terminal_read"
    }

    fn description(&self) -> &'static str {
        "Read a terminal on the user's stage (this session's): its last lines as text. `desk` \
         lists the terminals (`terminal:<id>`)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "the terminal's id (from terminal:<id>)" },
                "lines": { "type": "integer", "description": "how many of the last lines (default 60)" }
            },
            "required": ["id"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let terminal = terminal(context, &args)?;
        let lines = args["lines"]
            .as_u64()
            .map_or(LINES, |n| (n as usize).min(MAX_LINES));
        let text = terminal.text(lines);
        Ok(match text.is_empty() {
            true => format!("terminal {} shows nothing yet", terminal.id),
            false => format!(
                "terminal {} (in {}):\n{text}",
                terminal.id,
                terminal.cwd.display()
            ),
        })
    }
}
