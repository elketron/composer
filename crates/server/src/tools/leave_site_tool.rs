use async_trait::async_trait;
use serde_json::{Value, json};

use crate::agent::leave_site;
use crate::tools::{Tool, ToolContext};

pub struct LeaveSiteTool;

#[async_trait]
impl Tool for LeaveSiteTool {
    fn name(&self) -> &'static str {
        "leave_site"
    }

    fn description(&self) -> &'static str {
        "Leave the site you entered and return to the base tools."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, context: &ToolContext, _args: Value) -> Result<String, String> {
        Ok(match leave_site(&context.server, &context.session) {
            Some(site) => format!("left site://{site}"),
            None => "no site entered".into(),
        })
    }
}
