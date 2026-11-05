use std::sync::Arc;

use composer_protocol::ApprovalDecision;
use tokio_util::sync::CancellationToken;

use crate::actions::{Caller, Justfile, Recipe, Refusal};
use crate::server::Server;
use crate::session::Session;

/// Makes sure this version of `recipe` is approved for `caller`, asking the session's clients
/// when it is not, and answers `Ok` when it may run. Shared by the `run` tool and the HTTP route.
///
/// An approval a user gave is remembered. One an orchestrator or `[system1]` gave for a worker
/// holds for this run only, except in a worker process, which keeps approvals in memory and ends
/// with its ticket. (No test runs that worker branch: none sets up a parent link here. The
/// parent's side is covered by `a_delegated_answer_holds_for_one_run_and_a_users_is_remembered`.)
#[allow(clippy::too_many_arguments)]
pub async fn ensure_approved(
    server: &Arc<Server>,
    session: &Session,
    site: &str,
    justfile: &Justfile,
    recipe: &Recipe,
    args: &[String],
    caller: Caller,
    cancel: &CancellationToken,
) -> Result<(), Refusal> {
    let hash = justfile.hash(&recipe.name);
    if server.approvals.may_run(site, recipe, &hash, caller) {
        return Ok(());
    }
    let mut body = justfile.show(&recipe.name).await.map_err(Refusal::Denied)?;
    if caller == Caller::App {
        body = format!(
            "# An app asks to run this. Approving lets any app or paired device run this version \
             with any arguments, without asking.\n\n{body}"
        );
    } else if recipe.app_visible {
        body = format!(
            "# Marked [metadata('app')]. Approving also lets any app or paired device run this \
             version with any arguments, without asking.\n\n{body}"
        );
    }
    let answer = server
        .request_approval(
            session,
            site,
            &recipe.name,
            args,
            body,
            &hash,
            caller,
            cancel,
        )
        .await;
    match answer.map(|a| (a.decision, a.reason)) {
        Some((ApprovalDecision::Approve | ApprovalDecision::Always, reason)) => {
            if reason.is_none() || server.parent.is_some() {
                server
                    .approvals
                    .approve(site, &recipe.name, &hash, caller)
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
