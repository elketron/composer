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
        let servers: Vec<String> = crate::site_mcp::load(workspace, &site.name)
            .map(|file| file.servers.into_iter().map(|s| s.name).collect())
            .unwrap_or_default();
        let tools = if servers.is_empty() {
            tools
        } else {
            format!(
                "{tools}; its mcp.toml declares the MCP servers: {} (their tools are yours here \
                 once the user approved the file)",
                servers.join(", ")
            )
        };
        sections.push(format!(
            "# Site\n\nYou are in `site://{}` ({}); {tools}. `set_site` with no site leaves \
             it.",
            site.name, site.title
        ));
        if settings.folder.is_some()
            && let Ok(Some((url, instructions))) =
                folder_instructions(workspace, Some(&site.name), settings.folder.as_deref())
        {
            sections.push(format!("<page url=\"{url}\">\n{instructions}\n</page>"));
        }
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

/// A single shot's whole system prompt: the instructions of its folder in its site, and nothing
/// of composer's own. `None` (no system prompt) for a single shot in no site.
pub fn single_shot_prompt(
    workspace: &Web,
    site: Option<&str>,
    folder: Option<&str>,
) -> Option<String> {
    folder_instructions(workspace, site, folder)
        .ok()
        .flatten()
        .map(|(_, instructions)| instructions)
}

/// The instructions of `folder` (a workflow folder inside `site`; none: the site itself): its
/// `CONTEXT.md`, else its `index.md`, without frontmatter, and where they are. `Ok(None)` without a
/// site; an error for a folder outside the site, or one with neither file.
pub fn folder_instructions(
    workspace: &Web,
    site: Option<&str>,
    folder: Option<&str>,
) -> Result<Option<(SiteUrl, String)>, String> {
    let Some(site) = site else {
        return match folder {
            Some(folder) => Err(format!("the folder `{folder}` needs a site")),
            None => Ok(None),
        };
    };
    let folder = folder.map(|f| f.trim().trim_matches('/')).unwrap_or("");
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    };
    for file in ["CONTEXT.md", "index.md"] {
        let url = SiteUrl::new(site, &format!("{prefix}{file}")).map_err(|e| e.to_string())?;
        if let Ok(page) = workspace.read_page(&url) {
            return Ok(Some((page.url.clone(), page.body().trim().to_string())));
        }
    }
    Err(format!(
        "site://{site}/{prefix} has no CONTEXT.md or index.md"
    ))
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
