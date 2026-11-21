use std::path::Path;

use composer_core::{Index, SiteUrl, Web};
use composer_protocol::SessionInfo;

use crate::agent::AgentDefinition;

/// Memory pages listed in the prompt, at most.
const MEMORY_LIST_MAX: usize = 60;

/// The system prompt: agent body → `site://home` → environment → relevant memory → the active
/// project, which changes most often, last.
pub fn system_prompt(
    agent: &AgentDefinition,
    workspace: &Web,
    index: &Index,
    session: &SessionInfo,
) -> String {
    let mut sections = vec![agent.prompt.clone()];
    let site = session
        .site
        .as_deref()
        .and_then(|name| crate::agent::Site::load(workspace, name).ok());
    if let Some(site) = &site {
        sections.push(format!(
            "<page url=\"site://{}/\">\n{}\n</page>",
            site.name, site.prompt
        ));
        let tools = if site.tools.is_empty() {
            "it adds no tools".to_string()
        } else {
            format!("its tools are: {}", site.tools.join(", "))
        };
        let tools = if site.mcp.is_empty() {
            tools
        } else {
            format!(
                "{tools}; it also names the MCP servers: {} (their tools are yours here where \
                 config.toml lets this site grant them)",
                site.mcp.join(", ")
            )
        };
        sections.push(format!(
            "# Site\n\nYou are in `site://{}` ({}); {tools}. `leave_site` returns to the \
             base tools.",
            site.name, site.title
        ));
    } else if let Some(home) = page_body(workspace, "site://home") {
        sections.push(format!("<page url=\"site://home\">\n{home}\n</page>"));
    }

    sections.push(format!(
        "# Environment\n- you are the `{name}` agent ({url}); pages about other agents describe their roles, not yours\n- date: {date}\n- session: {id}",
        name = agent.name,
        url = agent.url,
        date = chrono::Local::now().format("%Y-%m-%d %A"),
        id = session.id,
    ));

    let mut memory = Vec::new();
    if let Some(user) = page_body(workspace, "site://memory/user") {
        memory.push(format!(
            "<page url=\"site://memory/user.md\">\n{user}\n</page>"
        ));
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
    // Last, as it changes most often: the sections before it stay a
    // stable prefix for the provider's prompt cache.
    sections.push(active_project(workspace, session));

    sections.join("\n\n---\n\n")
}

/// The active project and its page (`site://projects/<name>`, else `site://memory/projects/<name>`).
fn active_project(workspace: &Web, session: &SessionInfo) -> String {
    let Some(project) = &session.project else {
        return "# Active project\n\nNone.".to_string();
    };
    let mut section = format!(
        "# Active project\n\n{project}{}",
        git_branch(Path::new(project))
    );
    if let Some(name) = Path::new(project).file_name().and_then(|n| n.to_str()) {
        let url = format!("site://projects/{name}");
        let fallback = format!("site://memory/projects/{name}");
        match page_body(workspace, &url).or_else(|| page_body(workspace, &fallback)) {
            Some(page) => section.push_str(&format!("\n\n<page url=\"{url}.md\">\n{page}\n</page>")),
            None => section.push_str(&format!(
                "\n\nNo project page for this project yet: create `{url}.md` when you learn something worth keeping."
            )),
        }
    }
    section
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

fn page_body(workspace: &Web, url: &str) -> Option<String> {
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
