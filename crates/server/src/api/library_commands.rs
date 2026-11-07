//! `library.list`, `library.install`, `library.update`: the workspace's side of the library, run
//! by the server so recipe approvals land in its own (in-memory) approvals.

use composer_protocol::CommandResult;
use composer_protocol::http::{
    InstalledSiteInfo, LibraryChange, LibraryPublishPlan, LibrarySiteInfo,
};

use crate::library::{PublishPlan, SiteName, WorkspaceLibrary};
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

/// Without `approved`: the plan. With it (the plan's digest): the publish, of exactly that.
pub fn publish(
    server: &Server,
    site: &str,
    name: Option<&str>,
    approved: Option<&str>,
) -> Result<CommandResult, String> {
    let site = SiteName::new(site)?;
    let name = name.map(SiteName::new).transpose()?;
    let library = WorkspaceLibrary::of(&server.home)?;
    let Some(approved) = approved else {
        let plan = library.plan_publish(&site, name.as_ref())?;
        return Ok(CommandResult::LibraryPublishPlan {
            plan: plan_info(&plan),
        });
    };
    let plan = library.publish(&site, name.as_ref(), approved)?;
    let commit = library
        .installed()?
        .get(&site)
        .map(|from| from.commit.short().to_string())
        .unwrap_or_default();
    Ok(CommandResult::LibraryPublished {
        site: plan.site.to_string(),
        name: plan.name.to_string(),
        commit,
    })
}

pub fn plan_info(plan: &PublishPlan) -> LibraryPublishPlan {
    LibraryPublishPlan {
        site: plan.site.to_string(),
        name: plan.name.to_string(),
        new_site: plan.new_site,
        changes: plan
            .changes
            .iter()
            .map(|(path, kind)| LibraryChange {
                path: path.display().to_string(),
                outcome: kind.to_string(),
                review: None,
            })
            .collect(),
        left_out: plan
            .left_out
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        text: plan.describe(),
        digest: plan.digest.clone(),
    }
}
