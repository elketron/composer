//! One project's overview: git state, the works that link it, ticket branches still there, its
//! memory page, its harness switches.

use std::path::Path;

use composer_core::SiteUrl;
use composer_protocol::http::{PageExcerpt, ProjectOverview, TicketBranchSummary, WorkSummary};

use crate::project::TicketBranch;
use crate::project::git::git;
use crate::server::Server;
use crate::tools::{resolve_project, tilde};
use crate::worker::ticket::ticket_url;

/// Memory excerpts stop after this many characters.
const EXCERPT_CHARS: usize = 280;

/// `project` is a name in the projects directory or a path.
pub fn project_overview(server: &Server, project: &str) -> Result<ProjectOverview, String> {
    let folder = resolve_project(project, &server.config.projects.dir)?;
    let path = Path::new(&folder);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| folder.clone());
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

    Ok(ProjectOverview {
        works: works(server, &folder),
        branches: top.map(|top| branches(server, &top)).unwrap_or_default(),
        memory: memory(server, &name),
        harnesses: server.workers.switches(server, &folder),
        tool_rules: server.tool_rules.rules(&folder).len() as u32,
        path: tilde(path),
        name,
        folder,
        branch,
        uncommitted,
    })
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
                .and_then(|p| resolve_project(&p, &server.config.projects.dir).ok())
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
    let root = server.home.state().join("worktrees");
    let mut branches: Vec<TicketBranchSummary> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let ticket = entry.file_name().into_string().ok()?;
            let branch = TicketBranch::find(&root, &ticket).ok()??;
            if branch.main.canonicalize().ok()? != top {
                return None;
            }
            let page = ticket_url(&ticket)
                .ok()
                .and_then(|url| server.workspace.read_page(&url).ok());
            Some(TicketBranchSummary {
                title: page
                    .as_ref()
                    .map(|p| p.title())
                    .unwrap_or_else(|| ticket.clone()),
                status: page.as_ref().and_then(|p| p.frontmatter.get_str("status")),
                commits: branch.commit_count().unwrap_or(0),
                branch: branch.branch,
                ticket,
            })
        })
        .collect();
    branches.sort_by(|a, b| a.ticket.cmp(&b.ticket));
    branches
}

/// `site://memory/projects/<name>`: its first paragraphs, without headings and frontmatter.
fn memory(server: &Server, name: &str) -> Option<PageExcerpt> {
    let url = SiteUrl::new("memory", &format!("projects/{name}.md")).ok()?;
    let page = server.workspace.read_page(&url).ok()?;
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
        url: url.to_string(),
        excerpt,
    })
}
