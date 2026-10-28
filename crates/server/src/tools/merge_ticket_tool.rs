use std::sync::Arc;

use async_trait::async_trait;
use composer_protocol::{
    ApprovalDecision, ApprovalKind, ChatMessage, MessageSource, Role, TurnState,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::server::Server;
use crate::session::{Session, submit};
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
         asked and sees the changes; that can take hours, so this returns right away and a \
         message tells you later: merged (with the commit), or declined, or why it failed. \
         Meanwhile go on with tickets that don't need this one merged. Read the ticket's \
         Result before you ask."
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
        // The user may answer in hours: asked in the background, the outcome comes back as a
        // message, and the orchestrator goes on meanwhile.
        let title = format!("Merge ticket {ticket} into {target}");
        tokio::spawn(ask_and_merge(
            server.clone(),
            context.session.clone(),
            ticket.clone(),
            target.clone(),
            title,
            body,
        ));
        Ok(format!(
            "asked the user to merge ticket {ticket} into {target}; a message tells you when \
             they decide. Go on with work that doesn't need it merged."
        ))
    }
}

/// Asks the user to merge `ticket`, merges it if they approve, and tells `orchestrator`.
async fn ask_and_merge(
    server: Arc<Server>,
    orchestrator: Arc<Session>,
    ticket: String,
    target: String,
    title: String,
    body: String,
) {
    let decision = server
        .ask_approval(
            &orchestrator,
            ApprovalKind::Merge,
            title,
            body,
            None,
            &CancellationToken::new(),
        )
        .await;
    // Asked while a turn ran, which has likely ended since.
    let state = if orchestrator.is_running() {
        TurnState::Running
    } else {
        TurnState::Idle
    };
    orchestrator.update_info(&server.hub, |info| info.state = state);
    let outcome = match decision.map(|answer| answer.decision) {
        Some(ApprovalDecision::Approve | ApprovalDecision::Always) => {
            let merging = server.clone();
            let id = ticket.clone();
            match tokio::task::spawn_blocking(move || merge_ticket(&merging, &id)).await {
                Ok(Ok(commit)) => {
                    format!("merged into {target} as {commit}; the worktree and branch are removed")
                }
                Ok(Err(e)) => format!("not merged: {e}"),
                Err(e) => format!("not merged: {e}"),
            }
        }
        Some(ApprovalDecision::Deny) => "the user declined; it is not merged".into(),
        None => return,
    };
    submit(
        &server,
        &orchestrator,
        ChatMessage {
            role: Role::User,
            content: format!("[merge of ticket {ticket}: {outcome}]"),
            thinking: None,
            tool_calls: vec![],
            pages: vec![],
            images: vec![],
            attachments: vec![],
            source: Some(MessageSource::Server),
        },
    );
}
