//! A finished ticket's way back into its project: its changes, merging them, or discarding them.

use std::path::Path;

use composer_protocol::http::ProjectDiff;

use crate::project::{TicketBranch, project_diff};
use crate::server::Server;
use crate::worker::ticket::{set_ticket_fields, ticket_url};

/// The ticket's changes against where its branch left the target branch (`into`, default: the
/// project's checked-out branch; the ticket's `merge_into:` frontmatter counts as an `into`),
/// with what merging it would do.
pub async fn ticket_changes(
    server: &Server,
    ticket: &str,
    into: Option<&str>,
) -> Result<ProjectDiff, String> {
    let branch = ticket_branch(server, ticket)?;
    let into = merge_target(server, ticket, into);
    let base = branch.base(into.as_deref())?;
    let mut diff = project_diff(&branch.worktree, Some(&base)).await?;
    diff.ticket = Some(branch.review(into.as_deref())?);
    Ok(diff)
}

/// Merges the ticket's branch into the target branch (`into`, default: the ticket's
/// `merge_into:` frontmatter, else the project's checked-out branch) and removes its worktree
/// and branch. Returns the merge commit.
pub fn merge_ticket(server: &Server, ticket: &str, into: Option<&str>) -> Result<String, String> {
    let branch = ticket_branch(server, ticket)?;
    not_running(server, ticket)?;
    let title = ticket_title(server, ticket);
    let into = merge_target(server, ticket, into);
    let commit = branch.merge(&format!("Merge ticket {ticket}: {title}"), into.as_deref())?;
    set_ticket_fields(server, ticket, &[("merged", &commit)])?;
    Ok(commit)
}

/// Throws the ticket's branch and worktree away.
pub fn discard_ticket(server: &Server, ticket: &str) -> Result<(), String> {
    let branch = ticket_branch(server, ticket)?;
    not_running(server, ticket)?;
    branch.discard()?;
    set_ticket_fields(server, ticket, &[("discarded", "true")])
}

/// Whether the ticket's branch is gone already (merged or discarded): it can't be resumed.
pub fn ticket_closed(server: &Server, ticket: &str) -> Option<String> {
    let page = server.web.read_page(&ticket_url(ticket).ok()?).ok()?;
    if let Some(commit) = page.frontmatter.get_str("merged").filter(|c| !c.is_empty()) {
        return Some(format!("ticket {ticket} was merged ({commit})"));
    }
    (page.frontmatter.get_str("discarded").as_deref() == Some("true"))
        .then(|| format!("ticket {ticket} was discarded"))
}

/// Where this ticket's work lands: the explicit `into`, else its `merge_into:` frontmatter.
pub fn merge_target(server: &Server, ticket: &str, into: Option<&str>) -> Option<String> {
    into.map(|s| s.to_string()).or_else(|| {
        ticket_url(ticket)
            .ok()
            .and_then(|url| server.web.read_page(&url).ok())
            .and_then(|page| page.frontmatter.get_str("merge_into"))
            .filter(|branch| !branch.is_empty())
    })
}

fn ticket_branch(server: &Server, ticket: &str) -> Result<TicketBranch, String> {
    let root = server.home.state().join("worktrees");
    match TicketBranch::find(&root, ticket)? {
        Some(branch) => Ok(branch),
        None => Err(ticket_closed(server, ticket).unwrap_or_else(|| {
            format!(
                "ticket {ticket} has no worktree in {} (its project isn't a git checkout, or it \
                 was removed)",
                Path::new(&root).display()
            )
        })),
    }
}

fn not_running(server: &Server, ticket: &str) -> Result<(), String> {
    if server
        .workers
        .running()
        .iter()
        .any(|(_, handle)| handle.ticket == ticket)
    {
        return Err(format!(
            "ticket {ticket}'s worker is still running; wait for it or cancel it first"
        ));
    }
    Ok(())
}

fn ticket_title(server: &Server, ticket: &str) -> String {
    ticket_url(ticket)
        .ok()
        .and_then(|url| server.web.read_page(&url).ok())
        .map(|page| page.title())
        .unwrap_or_else(|| ticket.to_string())
}
