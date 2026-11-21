use std::sync::Arc;

use composer_protocol::{ApprovalDecision, ApprovalKind, ServerMessage};
use tokio_util::sync::CancellationToken;

use crate::actions::ApprovalAnswer;
use crate::server::Server;
use crate::session::Session;
use crate::worker::{Verdict, WorkerApprovals};

/// What deciding a worker's approval request without the user came to.
pub enum Delegated {
    /// Decided (`None`: cancelled first): the clients are not asked.
    Decided(Option<ApprovalAnswer>),
    /// The clients decide; the request may say who asks them.
    AskClients,
}

/// A worker's tool or recipe: `[system1]` first, then its orchestrator (when its agent decides
/// them, see [`WorkerApprovals`]). Anything else goes to the clients unchanged.
pub async fn delegate_approval(
    server: &Arc<Server>,
    session: &Session,
    request: &str,
    message: &mut ServerMessage,
    cancel: &CancellationToken,
) -> Delegated {
    let ServerMessage::ApprovalRequest {
        kind,
        title,
        body,
        site,
        recipe,
        args,
        ..
    } = message
    else {
        return Delegated::AskClients;
    };
    if !WorkerApprovals::delegable(server, session, *kind) {
        return Delegated::AskClients;
    }
    let what = match kind {
        ApprovalKind::Recipe => {
            format!("run recipe `{recipe}` of site://{site} {}", args.join(" "))
                .trim_end()
                .to_string()
        }
        _ => title.clone(),
    };
    let mut decided_by = server.config().system1.model.clone();
    let mut verdict = tokio::select! {
        verdict = crate::worker::system1::decide(server, session, &what, body) => verdict,
        _ = cancel.cancelled() => return Delegated::Decided(None),
    };
    if verdict.is_none()
        && let Some(orchestrator) = WorkerApprovals::decider(server, session, *kind)
    {
        decided_by = "Its orchestrator".into();
        verdict = server
            .worker_approvals
            .ask(server, &orchestrator, session, request, &what, body, cancel)
            .await;
        if verdict.is_none() {
            return Delegated::Decided(None);
        }
    }
    let decided = |decision, reason: String| {
        Delegated::Decided(Some(ApprovalAnswer {
            decision,
            reason: Some(reason),
        }))
    };
    match verdict {
        None => Delegated::AskClients,
        Some(Verdict::Approve(reason)) => decided(ApprovalDecision::Approve, reason),
        Some(Verdict::Deny(reason)) => decided(ApprovalDecision::Deny, reason),
        Some(Verdict::AskUser(reason)) => {
            *body = format!("{decided_by} asks you: {reason}\n\n{body}");
            Delegated::AskClients
        }
    }
}
