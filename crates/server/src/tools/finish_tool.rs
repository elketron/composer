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
        let state = match args.status.as_str() {
            "done" => WorkerState::Done,
            "failed" => WorkerState::Failed,
            other => return Err(format!("status must be done or failed, not `{other}`")),
        };
        if let Some(parent) = &context.server.parent {
            // A `composer worker` process: the parent finishes it.
            parent.finish(state, args.summary).await?;
            context.server.mark_worker_finished();
        } else if context.server.workers.get(context.session.id()).is_some() {
            // Another harness, calling through `tool.call`.
            context.server.workers.finish(
                &context.server,
                context.session.id(),
                state,
                args.summary,
            );
        } else {
            return Err("only workers finish; this session is not a worker".into());
        }
        Ok("finished: the orchestrator has been told. End your turn now.".into())
    }
}
