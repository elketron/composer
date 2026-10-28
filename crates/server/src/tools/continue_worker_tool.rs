use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::SteerWorkerArgs;
use crate::tools::{Tool, ToolContext};
use crate::worker::ticket::normalize_id;

pub struct ContinueWorkerTool;

#[async_trait]
impl Tool for ContinueWorkerTool {
    fn name(&self) -> &'static str {
        "continue_worker"
    }

    fn description(&self) -> &'static str {
        "Let a worker that reached its turn limit (tokens, or turns) go on with its ticket for another round. \
         `message` goes to it: what you see blocking it, a hint, or just to go on. Continue a \
         worker that is making progress; one going in circles needs a better hint, or the user. \
         Also sends a worker that finished (or stopped) back to its ticket with its context: \
         review findings to fix, a question about its Result. Prefer it to a new fix ticket: it \
         already knows the code."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ticket": { "type": "string", "description": "ticket id, e.g. 0003" },
                "message": { "type": "string", "description": "what the worker is told" },
                "compact": {
                    "type": "boolean",
                    "description": "compact its session first: it saves what matters and summarizes its old steps (own workers; not Claude Code)"
                }
            },
            "required": ["ticket", "message"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: SteerWorkerArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let server = &context.server;
        if server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot continue workers".into());
        }
        server
            .workers
            .go_on(server, &normalize_id(&args.ticket), args.message, args.compact)
            .await
    }
}
