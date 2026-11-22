//! The policy pages system-1 judges by: the home-wide `site://home/approvals.md` and the session
//! site's `<site>/approvals.md`.
//!
//! The agent being judged can write pages, so a policy counts only in a version the user approved:
//! the same request and store as a site's `mcp.toml`, keyed by a hash of the bytes. A version
//! nobody answered is asked about when system-1 first needs it, as a trust change (the user's
//! only; system-1 never answers it). A refused version doesn't count and isn't asked about again
//! until it changes (or the server restarts); meanwhile system-1 decides without it. An edit is a
//! new hash, so it goes back to the user.

use std::sync::Arc;

use composer_protocol::ApprovalDecision;
use tokio_util::sync::CancellationToken;

use crate::actions::Ask;
use crate::content_hash;
use crate::server::Server;
use crate::session::Session;

/// A policy page's name in every site, and what its approval is kept under.
pub const POLICY_FILE: &str = "approvals.md";

/// A policy page bigger than this is ignored (a policy is a page of rules, not a document).
const MAX_BYTES: usize = 16 * 1024;

/// A policy page in a version the user approved.
#[derive(Debug, Clone)]
pub struct Policy {
    pub url: String,
    pub text: String,
}

/// The policies that count for a session in `site`, the home's first, asking the user (in
/// `session`) about versions they never answered. `None` if `cancel` fired while asking.
pub async fn approved(
    server: &Arc<Server>,
    session: &Session,
    site: Option<&str>,
    cancel: &CancellationToken,
) -> Option<Vec<Policy>> {
    let mut sites = vec!["home"];
    if let Some(site) = site.filter(|s| *s != "home") {
        sites.push(site);
    }
    let mut policies = Vec::new();
    for site in sites {
        let path = server.home.sites().join(site).join(POLICY_FILE);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let url = format!("site://{site}/{POLICY_FILE}");
        if bytes.len() > MAX_BYTES {
            tracing::warn!("{url} is over {MAX_BYTES} bytes; system-1 ignores it");
            continue;
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if text.trim().is_empty() {
            continue;
        }
        let hash = content_hash(&bytes);
        let policy = Policy {
            url: url.clone(),
            text,
        };
        if server.approvals.is_approved(site, POLICY_FILE, &hash) {
            policies.push(policy);
            continue;
        }
        if server.system1.refused(site, &hash) {
            continue;
        }
        let ask = Ask::trust(site, POLICY_FILE, vec![], asking(&policy), &hash);
        match server.ask_user(session, &ask, cancel).await? {
            ApprovalDecision::Approve | ApprovalDecision::Always => {
                if let Err(e) = server.approvals.approve(site, POLICY_FILE, &hash) {
                    tracing::warn!("remembering that {url} is approved: {e}");
                }
                policies.push(policy);
            }
            ApprovalDecision::Deny => server.system1.refuse(site, &hash),
        }
    }
    Some(policies)
}

/// What the user reads: what the page is for, then the page itself, so the words they approve
/// are the words system-1 is given.
fn asking(policy: &Policy) -> String {
    format!(
        "{} is a policy for system-1, the model that approves or rejects the agent's actions for \
         you (terminal commands, background processes, MCP and outside agents' tool calls, recipe \
         runs). It counts only once you approve this version; after an edit you are asked \
         again. Until then system-1 decides without it.\n\n{}",
        policy.url, policy.text
    )
}
