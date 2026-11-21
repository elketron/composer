use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::{Tool, ToolContext};

/// Ends a workflow run: its `value` is the run's result.
pub struct FinishTool;

#[async_trait]
impl Tool for FinishTool {
    fn name(&self) -> &'static str {
        "finish"
    }

    fn description(&self) -> &'static str {
        "End this workflow run with its result: `value` is what the run returns (text, or any \
         JSON the task asks for). Nothing runs after it."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "value": { "description": "the run's result" }
            },
            "required": ["value"]
        })
    }

    async fn call(&self, _context: &ToolContext, args: Value) -> Result<String, String> {
        match args.get("value") {
            Some(_) => Ok("finished".into()),
            None => Err("give the result as `value`".into()),
        }
    }
}
