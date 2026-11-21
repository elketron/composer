//! The worker side of a connection: a worker process's events and approvals, and an external
//! harness's permission requests. Kept apart from [`crate::api::Connection`], whose approval path
//! (clients answering `approval.respond`) needs none of it.

use std::sync::Arc;

use composer_protocol::{ServerMessage, SessionId};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::actions::Caller;
use crate::harness::{PERMISSION_TOOL, WorkerSink, answer_permission};
use crate::server::Server;

/// An event from the worker process running `worker`, for its session here.
pub fn worker_event(server: &Arc<Server>, worker: Option<SessionId>, event: ServerMessage) {
    if let Some(sink) = worker.and_then(|id| WorkerSink::new(server, &id)) {
        sink.event(event);
    }
}

/// An external harness's permission request (`tool.call` of [`PERMISSION_TOOL`]): its answer, or
/// `None` when the call is no such request.
pub async fn permission_answer(
    server: &Arc<Server>,
    session: &str,
    name: &str,
    args: &Value,
) -> Option<String> {
    if name != PERMISSION_TOOL || !server.workers.get(session).is_some_and(|h| h.external) {
        return None;
    }
    Some(answer_permission(server, session, args.clone()).await)
}

/// A worker's recipe approval: already approved here, or asked of this server's clients (or
/// decided by its orchestrator: for this run only, with its reason).
pub async fn worker_approval(
    server: &Arc<Server>,
    worker: Option<SessionId>,
    site: &str,
    recipe: &str,
    args: &[String],
    body: String,
    hash: &str,
) -> Result<(bool, Option<String>), String> {
    if server
        .approvals
        .is_approved(site, recipe, hash, Caller::Agent)
    {
        return Ok((true, None));
    }
    let id = worker.ok_or("not a worker connection")?;
    let session = server.sessions.get(&id).map_err(|e| e.to_string())?;
    let decision = server
        .request_approval(
            &session,
            site,
            recipe,
            args,
            body,
            hash,
            Caller::Agent,
            &CancellationToken::new(),
        )
        .await;
    let approved = decision.as_ref().is_some_and(|answer| {
        matches!(
            answer.decision,
            composer_protocol::ApprovalDecision::Approve
                | composer_protocol::ApprovalDecision::Always
        )
    });
    let reason = decision.and_then(|answer| answer.reason);
    if approved && reason.is_none() {
        server
            .approvals
            .approve(site, recipe, hash, Caller::Agent)
            .map_err(|e| e.to_string())?;
    }
    Ok((approved, reason))
}
