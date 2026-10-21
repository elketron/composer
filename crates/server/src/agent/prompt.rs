use std::path::Path;

use composer_core::{Index, SiteUrl, Workspace};
use composer_protocol::SessionInfo;

use crate::agent::AgentDefinition;

/// Memory pages listed in the prompt, at most.
const MEMORY_LIST_MAX: usize = 60;

/// The system prompt: agent body → `site://home` → environment → relevant memory.
pub fn system_prompt(
    agent: &AgentDefinition,
    workspace: &Workspace,
    index: &Index,
    session: &SessionInfo,
) -> String {
    // A pipeline stage's worker gets its stage's scoped context instead of the usual one.
    if let Some(stage) = session.ticket.as_deref().and_then(|ticket| {
        crate::pipeline::stage_context(workspace, ticket, session.project.as_deref())
    }) {
        return format!("{}\n\n---\n\n{stage}", agent.prompt);
    }
    let mut sections = vec![agent.prompt.clone()];

    if let Some(home) = page_body(workspace, "site://home") {
        sections.push(format!("<page url=\"site://home\">\n{home}\n</page>"));
    }

    let mut env = format!(
        "# Environment\n- you are the `{name}` agent ({url}); pages about other agents describe their roles, not yours\n- date: {date}\n- session: {id}",
        name = agent.name,
        url = agent.url,
        date = chrono::Local::now().format("%Y-%m-%d %A"),
        id = session.id,
    );
    match &session.project {
        Some(project) => env.push_str(&format!(
            "\n- active project: {project}{}",
            git_branch(Path::new(project))
        )),
        None => env.push_str("\n- active project: none (set one with `set_project`)"),
    }
    if agent.allows_tool("spawn_worker") {
        let workers: Vec<String> = AgentDefinition::workers(workspace, index)
            .into_iter()
            .map(|w| format!("\n  - `{}`: {}", w.name, w.description))
            .collect();
        env.push_str(&format!(
            "\n- worker agents (`spawn_worker`'s `agent`):{}",
            if workers.is_empty() {
                " none".to_string()
            } else {
                workers.concat()
            }
        ));
    }
    sections.push(env);

    let mut memory = Vec::new();
    if let Some(user) = page_body(workspace, "site://memory/user") {
        memory.push(format!(
            "<page url=\"site://memory/user.md\">\n{user}\n</page>"
        ));
    }
    if let Some(name) = session
        .project
        .as_deref()
        .and_then(|p| Path::new(p).file_name())
        .and_then(|n| n.to_str())
    {
        let url = format!("site://memory/projects/{name}");
        match page_body(workspace, &url) {
            Some(project) => memory.push(format!("<page url=\"{url}.md\">\n{project}\n</page>")),
            None => memory.push(format!("No memory page for this project yet: create `{url}.md` when you learn something worth keeping.")),
        }
    }
    let pages = memory_pages(index);
    if !pages.is_empty() {
        memory.push(format!(
            "Memory pages (open the ones that matter for the task):\n{pages}"
        ));
    }
    if !memory.is_empty() {
        sections.push(format!("# Memory\n\n{}", memory.join("\n\n")));
    }

    sections.join("\n\n---\n\n")
}

/// `- title — url (kind)` for each memory page except the index.
fn memory_pages(index: &Index) -> String {
    let mut pages: Vec<String> = index
        .entries()
        .filter(|e| e.url.site() == "memory" && !e.url.path().ends_with("index.md"))
        .map(|e| match e.frontmatter.get_str("kind") {
            Some(kind) => format!("- {} — {} ({kind})", e.title, e.url),
            None => format!("- {} — {}", e.title, e.url),
        })
        .collect();
    if pages.len() > MEMORY_LIST_MAX {
        let more = pages.len() - MEMORY_LIST_MAX;
        pages.truncate(MEMORY_LIST_MAX);
        pages.push(format!("- … {more} more: `search site:memory <words>`"));
    }
    pages.join("\n")
}

fn page_body(workspace: &Workspace, url: &str) -> Option<String> {
    let url = SiteUrl::parse(url).ok()?;
    workspace
        .read_page(&url)
        .ok()
        .map(|p| p.body().trim().to_string())
}

/// ` (git branch main)`, or empty when not a git checkout.
fn git_branch(project: &Path) -> String {
    let head = std::fs::read_to_string(project.join(".git/HEAD")).unwrap_or_default();
    match head.trim().strip_prefix("ref: refs/heads/") {
        Some(branch) => format!(" (git branch {branch})"),
        None => String::new(),
    }
}
