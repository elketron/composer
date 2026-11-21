//! The gate that keeps an unapproved `mcp.toml` inert.
//!
//! A site file is agent-writable, and it holds the programs composer would run as the user, so it
//! is asked about the way a `justfile` recipe is: the same request, the same store keyed by a
//! hash of the bytes. Only a yes is remembered: a refused or unanswered file is asked about again
//! the next time the site is set, and an edit is a new hash, so it asks again too. The question
//! goes to the user's clients only; nothing answers it for them.

use std::sync::Arc;

use composer_protocol::ApprovalDecision;
use tokio_util::sync::CancellationToken;

use crate::server::Server;
use crate::session::Session;
use crate::site_mcp::{ApprovedMcp, MCP_FILE, Note, SiteMcpFile};

/// What the gate decided about one reading of one file.
#[derive(Debug, Clone)]
pub struct Gate {
    /// The servers the user said yes to. `None` when they refused, did not answer, could not be
    /// asked, or the file declares nothing usable.
    pub approved: Option<ApprovedMcp>,
    pub note: Note,
}

/// The file's servers when these exact bytes were approved before; never asks.
pub fn approved_now(server: &Server, site: &str, file: &SiteMcpFile) -> Option<ApprovedMcp> {
    (!file.servers.is_empty() && server.approvals.is_approved(site, MCP_FILE, &file.hash))
        .then(|| ApprovedMcp::new(site, &file.hash, file.servers.clone()))
}

/// Whether this version of `file` may be used, asking the user first (in `session`) when these
/// bytes have not been answered. Without a session (a run that is not stored) nobody can be
/// asked: only an earlier approval counts.
pub async fn ensure_approved(
    server: &Arc<Server>,
    session: Option<&Session>,
    site: &str,
    file: &SiteMcpFile,
    cancel: &CancellationToken,
) -> Gate {
    let note = |approved: bool| Note {
        approved,
        servers: file.servers.iter().map(|s| s.name.clone()).collect(),
        errors: file.errors.clone(),
    };
    if file.servers.is_empty() {
        return Gate {
            approved: None,
            note: note(false),
        };
    }
    if let Some(approved) = approved_now(server, site, file) {
        return Gate {
            approved: Some(approved),
            note: note(true),
        };
    }
    let Some(session) = session else {
        return Gate {
            approved: None,
            note: note(false),
        };
    };
    let answer = server
        .request_approval(
            session,
            site,
            MCP_FILE,
            &[],
            asking(site, file),
            &file.hash,
            cancel,
        )
        .await;
    let yes = matches!(
        answer,
        Some(ApprovalDecision::Approve | ApprovalDecision::Always)
    );
    if yes && let Err(e) = server.approvals.approve(site, MCP_FILE, &file.hash) {
        tracing::warn!("remembering that {site}'s {MCP_FILE} is approved: {e}");
    }
    Gate {
        approved: yes.then(|| ApprovedMcp::new(site, &file.hash, file.servers.clone())),
        note: note(yes),
    }
}

/// What the user reads: each server with what it would start or reach, then the file itself, so
/// the words they approve are the words the site runs.
fn asking(site: &str, file: &SiteMcpFile) -> String {
    let mut body = format!(
        "site://{site}'s `{MCP_FILE}` declares MCP servers that sessions in the site would use. \
         Nothing in it starts until you approve the file; the approval holds until the file is \
         edited.\n\n"
    );
    for server in &file.servers {
        body.push_str(&format!("- {}\n", server.describes()));
    }
    if !file.errors.is_empty() {
        body.push_str("\nNot understood, and left out whichever way you answer:\n");
        for error in &file.errors {
            body.push_str(&format!("- {error}\n"));
        }
    }
    // The file itself, whole: `parse` refuses one too big to show.
    body.push_str(&format!("\n{}\n{}", file.path.display(), file.text));
    body
}
