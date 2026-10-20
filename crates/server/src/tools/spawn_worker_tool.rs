use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::SpawnWorkerArgs;
use crate::tools::{Tool, ToolContext};
use crate::worker::ticket::normalize_id;

pub struct SpawnWorkerTool;

#[async_trait]
impl Tool for SpawnWorkerTool {
    fn name(&self) -> &'static str {
        "spawn_worker"
    }

    fn description(&self) -> &'static str {
        "Start a worker agent on a ticket (site://orchestration/tickets/<id>). The ticket becomes \
         `claimed`; the worker runs in its own session and process. You get a message when it \
         finishes; then read the ticket's Result. Returns immediately. `agent` picks which worker \
         agent (listed in your Environment); default: the ticket's `agent:`, else `worker`."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ticket": { "type": "string", "description": "ticket id, e.g. 0003" },
                "agent": { "type": "string", "description": "worker agent, e.g. coder (optional)" }
            },
            "required": ["ticket"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: SpawnWorkerArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let ticket = normalize_id(&args.ticket);
        if context.server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot start workers".into());
        }
        let (worker, worktree) = context.server.workers.spawn(
            &context.server,
            &context.session,
            &ticket,
            args.agent.as_deref(),
        )?;
        let agent = context
            .server
            .sessions
            .get(&worker)
            .map(|s| s.info().agent)
            .unwrap_or_default();
        let place = match worktree {
            Some(w) => format!(
                " It works on branch `{}` in its own worktree ({}), from the project's last \
                 commit: uncommitted changes in the project are not in it.",
                w.branch,
                w.path.display()
            ),
            None => String::new(),
        };
        Ok(format!(
            "worker #{worker} ({agent}) started on ticket {ticket} (now claimed).{place} You will \
             get a message when it finishes; don't wait for it in this turn."
        ))
    }
}
