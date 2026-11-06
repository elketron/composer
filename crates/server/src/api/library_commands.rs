//! `library.list`, `library.install`, `library.update`: the workspace's side of the library, run
//! by the server so recipe approvals land in its own (in-memory) approvals.

use composer_protocol::CommandResult;
use composer_protocol::http::{InstalledSiteInfo, LibraryChange, LibrarySiteInfo};

use crate::library::{SiteName, WorkspaceLibrary};
use crate::server::Server;
use crate::update::{UpdateItem, UpdateOutcome};

pub fn list(server: &Server) -> Result<CommandResult, String> {
    let sites = WorkspaceLibrary::of(&server.home)?
        .list()?
        .into_iter()
        .map(|state| LibrarySiteInfo {
            site: state.site.to_string(),
            commit: state.commit.short().to_string(),
            installed: state
                .installed
                .into_iter()
                .map(|(site, behind)| InstalledSiteInfo {
                    site: site.to_string(),
                    behind,
                })
                .collect(),
        })
        .collect();
    Ok(CommandResult::Library { sites })
}

pub async fn install(
    server: &Server,
    site: &str,
    name: Option<&str>,
) -> Result<CommandResult, String> {
    let site = SiteName::new(site)?;
    let name = name.map(SiteName::new).transpose()?;
    let installed = WorkspaceLibrary::of(&server.home)?
        .install(&site, name.as_ref(), &server.approvals)
        .await?;
    Ok(CommandResult::LibraryInstalled {
        site: installed.site.to_string(),
        from: installed.from.to_string(),
        commit: installed.commit.short().to_string(),
        files: installed.files,
        has_ui: installed.has_ui,
    })
}

pub async fn update(server: &Server, dry_run: bool) -> Result<CommandResult, String> {
    let items = WorkspaceLibrary::of(&server.home)?
        .update(dry_run, &server.approvals)
        .await?;
    Ok(CommandResult::LibraryUpdated {
        changes: items.into_iter().map(change).collect(),
    })
}

fn change(item: UpdateItem) -> LibraryChange {
    let (outcome, review) = match item.outcome {
        UpdateOutcome::Added => ("added", None),
        UpdateOutcome::Replaced => ("replaced", None),
        UpdateOutcome::Removed => ("removed", None),
        UpdateOutcome::Merged => ("merged", None),
        UpdateOutcome::Conflict { review } => ("conflict", Some(review)),
        UpdateOutcome::Review { review } => ("review", Some(review)),
    };
    LibraryChange {
        path: item.path,
        outcome: outcome.into(),
        review: review.map(|p| p.display().to_string()),
    }
}
