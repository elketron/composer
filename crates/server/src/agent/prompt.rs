use std::path::Path;

use composer_core::{Index, SiteUrl, Web};
use composer_protocol::SessionInfo;

use crate::agent::AgentDefinition;

/// Memory pages listed in the prompt, at most.
const MEMORY_LIST_MAX: usize = 60;

/// The system prompt: agent body → `site://home` → environment → relevant memory.
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
    let stage = session.ticket.as_deref().and_then(|ticket| {
        crate::pipeline::stage_context(workspace, ticket, session.project.as_deref())
    });

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
        let mode = site
            .project_mode
            .as_deref()
            .map_or_else(String::new, |mode| {
                format!(" Its works' project mode is `{mode}`.")
            });
        sections.push(format!(
            "# Site\n\nYou are in `site://{}` ({}); {tools}.{mode} `leave_site` returns to the \
             base tools.",
            site.name, site.title
        ));
    } else if stage.is_none()
        && let Some(home) = page_body(workspace, "site://home")
    {
        sections.push(format!("<page url=\"site://home\">\n{home}\n</page>"));
    }

    // A pipeline stage's worker gets its stage's scoped context instead of the usual environment.
    if let Some(stage) = stage {
        sections.push(stage);
        return sections.join("\n\n---\n\n");
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
        let url = format!("site://projects/{name}");
        let fallback = format!("site://memory/projects/{name}");
        match page_body(workspace, &url).or_else(|| page_body(workspace, &fallback)) {
            Some(project) => memory.push(format!("<page url=\"{url}.md\">\n{project}\n</page>")),
            None => memory.push(format!(
                "No project page for this project yet: create `{url}.md` when you learn something worth keeping."
            )),
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
