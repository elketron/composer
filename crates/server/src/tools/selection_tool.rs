use async_trait::async_trait;
use serde_json::{Value, json};

use crate::session::selection_context;
use crate::tools::{Tool, ToolContext};

/// What the user points at in an app on their stage (ticket 0175).
pub struct SelectionTool;

#[async_trait]
impl Tool for SelectionTool {
    fn name(&self) -> &'static str {
        "selection"
    }

    fn description(&self) -> &'static str {
        "What the user is pointing at in an app on their stage right now (the ticket, the hunk, \
         the part selected), as the app describes it. Their next message carries it too, unless \
         they removed it."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, context: &ToolContext, _args: Value) -> Result<String, String> {
        Ok(match context.session.selection() {
            Some(selection) => selection_context(&selection).trim().to_string(),
            None => "Nothing is selected in the apps on the user's stage.".into(),
        })
    }
}
