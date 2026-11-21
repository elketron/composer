use std::path::Path;

use composer_core::{Index, SiteUrl, Web};

use crate::agent::RunSettings;

/// Memory pages listed in the prompt, at most.
const MEMORY_LIST_MAX: usize = 60;

/// The system prompt: the run's prompt → the site or `site://home` → environment → relevant memory → the
/// site's folder, which changes most often, last.
pub fn system_prompt(
    settings: &RunSettings,
    workspace: &Web,
    index: &Index,
    site: Option<&str>,
    id: &str,
) -> String {
    let mut sections = vec![settings.prompt.clone()];
    let site = site.and_then(|name| crate::agent::Site::load(workspace, name).ok());
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
            "# Site\n\nYou are in `site://{}` ({}); {tools}. `set_site` with no site leaves \
             it.",
            site.name, site.title
        ));
    } else if let Some(home) = page_body(workspace, "site://home") {
        sections.push(format!("<page url=\"site://home\">\n{home}\n</page>"));
    }

    sections.push(format!(
        "# Environment\n- run: {kind}\n- date: {date}\n- session: {id}",
        kind = match settings.kind {
            composer_protocol::RunKind::SingleShot => "single shot",
            composer_protocol::RunKind::Workflow => "workflow (read-only)",
            composer_protocol::RunKind::Agentic => "agentic",
        },
        date = chrono::Local::now().format("%Y-%m-%d %A"),
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
    sections.push(folder_section(site.as_ref()));

    sections.join("\n\n---\n\n")
}

/// The folder of the session's site, if it names one, with its git branch.
fn folder_section(site: Option<&crate::agent::Site>) -> String {
    match site.and_then(|site| site.existing_folder().ok()) {
        Some(folder) => format!(
            "# Folder\n\nThe site works on {}{}: terminals start there.",
            folder.display(),
            git_branch(&folder)
        ),
        None => "# Folder\n\nNone.".to_string(),
    }
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
fn git_branch(folder: &Path) -> String {
    let head = std::fs::read_to_string(folder.join(".git/HEAD")).unwrap_or_default();
    match head.trim().strip_prefix("ref: refs/heads/") {
        Some(branch) => format!(" (git branch {branch})"),
        None => String::new(),
    }
}
