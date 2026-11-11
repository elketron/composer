//! One project's overview: git state, the works that link it, ticket branches still there, its
//! memory page, its harness switches.

use std::path::Path;

use composer_core::SiteUrl;
use composer_protocol::http::{
    AppInfo, PageExcerpt, ProjectOverview, ProjectSiteSummary, TicketBranchSummary, WorkSummary,
};

use crate::project::ProjectRef;
use crate::project::TicketBranch;
use crate::project::git::git;
use crate::server::Server;
use crate::tools::tilde;
use crate::worker::ticket::TicketId;
use crate::worker::ticket::ticket_url;

/// Memory excerpts stop after this many characters.
const EXCERPT_CHARS: usize = 280;

/// `project` is a name in the projects directory or a path.
pub fn project_overview(server: &Server, project: &str) -> Result<ProjectOverview, String> {
    let project = ProjectRef::resolve(server, project)?;
    let folder = project.root.display().to_string();
    let path = Path::new(&folder);
    let name = project.name.clone();
    let top = git(path, &["rev-parse", "--show-toplevel"])
        .ok()
        .and_then(|t| Path::new(&t).canonicalize().ok());
    let branch = top
        .as_ref()
        .and_then(|_| git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok());
    let uncommitted = top
        .as_ref()
        .and_then(|_| git(path, &["status", "--porcelain"]).ok())
        .map_or(0, |s| s.lines().filter(|l| !l.is_empty()).count() as u32);

    let works = works(server, &folder);
    let sites = site_summaries(server, &works, &project);
    Ok(ProjectOverview {
        page: project.page_url(),
        works,
        sites,
        branches: top.map(|top| branches(server, &top)).unwrap_or_default(),
        memory: memory(server, &name, &project),
        harnesses: server.workers.switches(server, &folder),
        tool_rules: server.tool_rules.rules(&folder).len() as u32,
        path: tilde(path),
        name,
        folder,
        branch,
        uncommitted,
    })
}

/// The sites operating on a project: the page's `sites:`, plus every site with a work on it.
fn site_summaries(
    server: &Server,
    works: &[WorkSummary],
    project: &ProjectRef,
) -> Vec<ProjectSiteSummary> {
    let apps = crate::apps::project_home(server, &project.name).ok();
    let mut names: Vec<String> = project.sites.clone();
    for work in works {
        if !names.contains(&work.pipeline) {
            names.push(work.pipeline.clone());
        }
    }
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|site| {
            let site_works: Vec<WorkSummary> = works
                .iter()
                .filter(|w| w.pipeline == site)
                .cloned()
                .collect();
            let running = site_works
                .iter()
                .flat_map(|w| &w.stages)
                .filter(|s| s.status == "running")
                .count() as u32;
            let waiting = site_works
                .iter()
                .flat_map(|w| &w.stages)
                .filter(|s| s.status == "review")
                .count() as u32;
            let title = SiteUrl::new(&site, "")
                .ok()
                .and_then(|url| server.web.read_page(&url).ok())
                .map(|page| page.title())
                .unwrap_or_else(|| site.clone());
            let app: Option<AppInfo> = apps
                .as_ref()
                .and_then(|list| list.apps.iter().find(|a| a.site == site).cloned());
            ProjectSiteSummary {
                site,
                title,
                works: site_works,
                running,
                waiting,
                app,
            }
        })
        .collect()
}

/// `type: work` pages whose `project:` is this folder.
fn works(server: &Server, folder: &str) -> Vec<WorkSummary> {
    let index = server.index.read().expect("index lock");
    let mut works: Vec<WorkSummary> = index
        .entries()
        .filter(|e| e.frontmatter.get_str("type").as_deref() == Some("work"))
        .filter(|e| {
            e.frontmatter
                .get_str("project")
                .and_then(|p| server.see_project(&p).ok())
                .as_deref()
                == Some(folder)
        })
        .map(|e| {
            // A work in a pipeline shows its stages; any other `type: work` page its `status:`.
            crate::pipeline::work_summary(server, &e.url).unwrap_or_else(|| WorkSummary {
                url: e.url.to_string(),
                pipeline: e.url.site().to_string(),
                title: e.title.clone(),
                status: e.frontmatter.get_str("status"),
                stages: vec![],
                project: e.frontmatter.get_str("project"),
            })
        })
        .collect();
    works.sort_by(|a, b| a.url.cmp(&b.url));
    works
}

/// Ticket worktrees of this repository that are still there.
fn branches(server: &Server, top: &Path) -> Vec<TicketBranchSummary> {
    let root = server.home.worktrees();
    let mut branches: Vec<TicketBranchSummary> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            // A folder whose name isn't a ticket id is no ticket's worktree.
            let ticket = TicketId::parse(&entry.file_name().into_string().ok()?).ok()?;
            let branch = TicketBranch::find(&root, &ticket).ok()??;
            if branch.main.canonicalize().ok()? != top {
                return None;
            }
            let page = ticket_url(&ticket)
                .ok()
                .and_then(|url| server.web.read_page(&url).ok());
            Some(TicketBranchSummary {
                title: page
                    .as_ref()
                    .map(|p| p.title())
                    .unwrap_or_else(|| ticket.to_string()),
                status: page.as_ref().and_then(|p| p.frontmatter.get_str("status")),
                commits: branch.commit_count().unwrap_or(0),
                branch: branch.branch,
                ticket: ticket.to_string(),
            })
        })
        .collect();
    branches.sort_by(|a, b| a.ticket.cmp(&b.ticket));
    branches
}

/// The project page's body, or the old `site://memory/projects/<name>` as a fallback: its first
/// paragraphs, without headings and frontmatter.
fn memory(server: &Server, name: &str, project: &ProjectRef) -> Option<PageExcerpt> {
    let page = project
        .page
        .as_ref()
        .and_then(|url| server.web.read_page(url).ok())
        .or_else(|| {
            let url = SiteUrl::new("memory", &format!("projects/{name}.md")).ok()?;
            server.web.read_page(&url).ok()
        })?;
    let body = page.body();
    let text: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let joined = text.join(" ");
    let excerpt = match joined.char_indices().nth(EXCERPT_CHARS) {
        Some((i, _)) => format!("{}…", &joined[..i]),
        None => joined,
    };
    Some(PageExcerpt {
        url: page.url.to_string(),
        excerpt,
    })
}
