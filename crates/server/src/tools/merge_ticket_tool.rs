use async_trait::async_trait;
use composer_protocol::{ApprovalDecision, ApprovalKind};
use serde_json::{Value, json};

use crate::tools::args::MergeTicketArgs;
use crate::tools::{Tool, ToolContext};
use crate::worker::ticket::normalize_id;
use crate::worker::{merge_ticket, ticket_changes};

pub struct MergeTicketTool;

#[async_trait]
impl Tool for MergeTicketTool {
    fn name(&self) -> &'static str {
        "merge_ticket"
    }

    fn description(&self) -> &'static str {
        "Merge a finished ticket's branch (`ticket/<id>`, from its own worktree) into the \
         project's checked-out branch, then remove the worktree and the branch. The user is \
         asked first and sees the changes; returns the merge commit, or why it didn't merge \
         (denied, conflicts, the worker still running). Read the ticket's Result before you ask."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ticket": { "type": "string", "description": "ticket id, e.g. 0003" }
            },
            "required": ["ticket"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: MergeTicketArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let ticket = normalize_id(&args.ticket);
        let server = &context.server;
        if server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot merge tickets".into());
        }
        let changes = ticket_changes(server, &ticket).await?;
        let review = changes.ticket.as_ref().ok_or("not a ticket worktree")?;
        let target = review
            .target
            .clone()
            .ok_or("the project has no branch checked out (detached HEAD)")?;
        if !review.conflicts.is_empty() {
            return Err(format!(
                "ticket {ticket} conflicts with {target} in: {}. Send its worker a message to \
                 merge {target} into its branch and resolve them, or tell the user.",
                review.conflicts.join(", ")
            ));
        }
        if review.ahead == 0 {
            return Err(format!("ticket {ticket} has nothing to merge"));
        }

        let mut body = format!(
            "{} → {target} · {} changed file(s)\n",
            review.branch,
            changes.files.len()
        );
        for file in &changes.files {
            body.push_str(&format!(
                "\n{} {}  +{} −{}",
                file.status, file.path, file.added, file.removed
            ));
        }
        if !review.commits.is_empty() {
            body.push_str("\n\nCommits:\n");
            body.push_str(&review.commits.join("\n"));
        }
        if review.uncommitted {
            body.push_str("\n\nThe worker left changes uncommitted; they are committed first.");
        }
        let decision = server
            .ask_approval(
                &context.session,
                ApprovalKind::Merge,
                format!("Merge ticket {ticket} into {target}"),
                body,
                None,
                &context.cancel,
            )
            .await;
        match decision {
            Some(ApprovalDecision::Approve | ApprovalDecision::Always) => {}
            Some(ApprovalDecision::Deny) => {
                return Err(format!("the user did not merge ticket {ticket}"));
            }
            None => return Err("cancelled".into()),
        }
        let server = server.clone();
        let commit = tokio::task::spawn_blocking(move || merge_ticket(&server, &ticket))
            .await
            .map_err(|e| e.to_string())??;
        Ok(format!(
            "merged into {target} as {commit}; the worktree and branch are removed"
        ))
    }
}
