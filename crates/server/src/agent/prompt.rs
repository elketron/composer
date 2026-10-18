use std::path::Path;

use composer_core::{SiteUrl, Workspace};
use composer_protocol::SessionInfo;

use crate::agent::AgentDefinition;

/// The system prompt: agent body → `site://home` → environment → relevant memory.
pub fn system_prompt(
    agent: &AgentDefinition,
    workspace: &Workspace,
    session: &SessionInfo,
) -> String {
    let mut sections = vec![agent.prompt.clone()];

    if let Some(home) = page_body(workspace, "site://home") {
        sections.push(format!("<page url=\"site://home\">\n{home}\n</page>"));
    }

    let mut env = format!(
        "# Environment\n- date: {}\n- session: {} (agent `{}`)",
        chrono::Local::now().format("%Y-%m-%d %A"),
        session.id,
        agent.name
    );
    match &session.project {
        Some(project) => env.push_str(&format!(
            "\n- active project: {project}{}",
            git_branch(Path::new(project))
        )),
        None => env.push_str("\n- active project: none (set one with `set_project`)"),
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
    if !memory.is_empty() {
        sections.push(format!("# Memory\n\n{}", memory.join("\n\n")));
    }

    sections.join("\n\n---\n\n")
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
