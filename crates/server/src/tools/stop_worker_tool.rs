use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::SteerWorkerArgs;
use crate::tools::{Tool, ToolContext};
use crate::worker::ticket::normalize_id;

pub struct StopWorkerTool;

#[async_trait]
impl Tool for StopWorkerTool {
    fn name(&self) -> &'static str {
        "stop_worker"
    }

    fn description(&self) -> &'static str {
        "Stop a worker that reached its turn limit: its ticket fails with `message` as the reason. \
         Change the ticket before you start it again."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ticket": { "type": "string", "description": "ticket id, e.g. 0003" },
                "message": { "type": "string", "description": "why it stops" }
            },
            "required": ["ticket", "message"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: SteerWorkerArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let server = &context.server;
        if server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot stop workers".into());
        }
        server
            .workers
            .stop_stuck(server, &normalize_id(&args.ticket), &args.message)
    }
}
