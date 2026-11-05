use std::sync::Arc;

use composer_protocol::ApprovalDecision;
use tokio_util::sync::CancellationToken;

use crate::actions::{Justfile, Recipe, Refusal};
use crate::server::Server;
use crate::session::Session;

/// Makes sure this version of `recipe` is approved to run, asking the session's clients when it
/// is not, and answers `Ok` when it may run. Shared by the `run` tool and the HTTP route.
///
/// An approval a user gave is remembered. One an orchestrator or `[system1]` gave for a worker
/// holds for this run only, unless `remember_delegated` (a worker process keeps approvals in
/// memory, so remembering there is the same thing).
#[allow(clippy::too_many_arguments)]
pub async fn ensure_approved(
    server: &Arc<Server>,
    session: &Session,
    site: &str,
    justfile: &Justfile,
    recipe: &Recipe,
    args: &[String],
    remember_delegated: bool,
    cancel: &CancellationToken,
) -> Result<(), Refusal> {
    let hash = justfile.hash(&recipe.name);
    if server.approvals.is_approved(site, &recipe.name, &hash) {
        return Ok(());
    }
    let body = justfile.show(&recipe.name).await.map_err(Refusal::Denied)?;
    let answer = server
        .request_approval(session, site, &recipe.name, args, body, &hash, cancel)
        .await;
    match answer.map(|a| (a.decision, a.reason)) {
        Some((ApprovalDecision::Approve | ApprovalDecision::Always, reason)) => {
            if reason.is_none() || remember_delegated {
                server
                    .approvals
                    .approve(site, &recipe.name, &hash)
                    .map_err(|e| Refusal::Denied(e.to_string()))?;
            }
        }
        Some((ApprovalDecision::Deny, Some(reason))) => {
            return Err(Refusal::Denied(format!(
                "your orchestrator denied running `{}`: {reason}",
                recipe.name
            )));
        }
        Some((ApprovalDecision::Deny, None)) => {
            return Err(Refusal::Denied(format!(
                "the user denied running `{}`; ask them why before trying again",
                recipe.name
            )));
        }
        None => return Err(Refusal::Denied("cancelled by the user".into())),
    }
    // The justfile may have changed while the user was looking: run only what they approved.
    let current = Justfile::load(justfile.dir())
        .await
        .map_err(Refusal::Denied)?;
    if current.hash(&recipe.name) != hash {
        return Err(Refusal::Changed(
            "the recipe changed while waiting for approval; run it again".into(),
        ));
    }
    Ok(())
}
