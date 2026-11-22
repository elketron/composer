use std::sync::Arc;

use composer_protocol::ApprovalDecision;
use tokio_util::sync::CancellationToken;

use crate::actions::{Answer, Ask, DecidedBy, Justfile, Recipe, Refusal};
use crate::server::Server;
use crate::session::Session;

/// Makes sure this version of `recipe` is approved, asking the session's clients when it is not,
/// and answers `Ok` when it may run. An approval the user gave is remembered; system-1's (see
/// [`crate::system1`]) is for this run only.
pub async fn ensure_approved(
    server: &Arc<Server>,
    session: &Session,
    site: &str,
    justfile: &Justfile,
    recipe: &Recipe,
    args: &[String],
    cancel: &CancellationToken,
) -> Result<(), Refusal> {
    let hash = justfile.hash(&recipe.name);
    if server.approvals.is_approved(site, &recipe.name, &hash) {
        return Ok(());
    }
    let body = justfile.show(&recipe.name).await.map_err(Refusal::Denied)?;
    let ask = Ask::recipe_run(site, &recipe.name, args, body, &hash);
    let what = ask.what.clone();
    match server.ask(session, ask, cancel).await {
        // Only the user's yes is remembered: system-1's holds for this run.
        Some(Answer {
            decision: ApprovalDecision::Approve | ApprovalDecision::Always,
            by,
        }) => {
            if by == DecidedBy::User {
                server
                    .approvals
                    .approve(site, &recipe.name, &hash)
                    .map_err(|e| Refusal::Denied(e.to_string()))?;
            }
        }
        Some(answer) => {
            return Err(Refusal::Denied(answer.refusal(&what, || {
                format!(
                    "the user denied running `{}`; ask them why before trying again",
                    recipe.name
                )
            })));
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
