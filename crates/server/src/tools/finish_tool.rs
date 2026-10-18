use async_trait::async_trait;
use composer_protocol::WorkerState;
use serde_json::{Value, json};

use crate::tools::args::FinishArgs;
use crate::tools::{Tool, ToolContext};

pub struct FinishTool;

#[async_trait]
impl Tool for FinishTool {
    fn name(&self) -> &'static str {
        "finish"
    }

    fn description(&self) -> &'static str {
        "Workers only: end your ticket. Write the ticket's Result section first. status is `done` \
         when every \"Done when\" item holds, else `failed` with the reason in summary."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["done", "failed"] },
                "summary": { "type": "string", "description": "one or two sentences for the orchestrator" }
            },
            "required": ["status", "summary"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: FinishArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let Some(parent) = &context.server.parent else {
            return Err("only workers finish; this session is not a worker".into());
        };
        let state = match args.status.as_str() {
            "done" => WorkerState::Done,
            "failed" => WorkerState::Failed,
            other => return Err(format!("status must be done or failed, not `{other}`")),
        };
        parent.finish(state, args.summary).await?;
        context.server.mark_worker_finished();
        Ok("finished: the orchestrator has been told. End your turn now.".into())
    }
}
