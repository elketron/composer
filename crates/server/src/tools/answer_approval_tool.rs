use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::AnswerApprovalArgs;
use crate::tools::{Tool, ToolContext};
use crate::worker::Verdict;
use crate::worker::ticket::normalize_id;

pub struct AnswerApprovalTool;

#[async_trait]
impl Tool for AnswerApprovalTool {
    fn name(&self) -> &'static str {
        "answer_approval"
    }

    fn description(&self) -> &'static str {
        "Decide a worker's approval request (a shell command, tool or recipe it wants to run) \
         instead of the user. `approve` lets it run once; `deny` refuses it, and `reason` goes \
         to the worker; `ask_user` hands it to the user, with `reason` as what they should weigh."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ticket": { "type": "string", "description": "the worker's ticket id, e.g. 0003" },
                "decision": { "type": "string", "enum": ["approve", "deny", "ask_user"] },
                "reason": { "type": "string", "description": "why, in one sentence" },
                "request": {
                    "type": "string",
                    "description": "the request id from the message; needed when several wait on one ticket"
                }
            },
            "required": ["ticket", "decision", "reason"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: AnswerApprovalArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let server = &context.server;
        if server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot answer approvals".into());
        }
        let verdict = match args.decision.as_str() {
            "approve" => Verdict::Approve(args.reason),
            "deny" => Verdict::Deny(args.reason),
            "ask_user" => Verdict::AskUser(args.reason),
            other => return Err(format!("decision `{other}`: approve, deny or ask_user")),
        };
        server.worker_approvals.answer(
            context.session.id(),
            &normalize_id(&args.ticket),
            args.request.as_deref(),
            verdict,
        )
    }
}
