use async_trait::async_trait;
use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::tools::args::NewProjectArgs;
use crate::tools::markdown_sections::{add_section, append_list_item, append_table_row};
use crate::tools::scaffold::copy_template;
use crate::tools::{Tool, ToolContext, expand_home, tilde};

/// Replaced by the project name in every template file.
const PLACEHOLDER: &str = "__PROJECT__";

pub struct NewProjectTool;

#[async_trait]
impl Tool for NewProjectTool {
    fn name(&self) -> &'static str {
        "new_project"
    }

    fn description(&self) -> &'static str {
        "Create a project: a new folder in the user's projects directory, its project page \
         (site://projects/<name>, type: project) and its entries in the projects index, Home and \
         the memory registry. With `template`, the folder starts as a copy of that site's \
         templates/project/ folder; otherwise it gets a README. Never touches an existing folder. \
         It becomes this session's active project."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "folder name: letters, digits, -, _ or ., e.g. desk-lamp" },
                "about": { "type": "string", "description": "one line: what the project is" },
                "template": { "type": "string", "description": "optional: a site with templates/project/, e.g. designer" }
            },
            "required": ["name", "about"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: NewProjectArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let name = args.name.trim();
        check_name(name)?;
        let about = one_line(&args.about);
        let server = &context.server;
        let page_url =
            SiteUrl::new("projects", &format!("{name}.md")).map_err(|e| e.to_string())?;
        let index_url = SiteUrl::new("projects", "").map_err(|e| e.to_string())?;
        let memory_url =
            SiteUrl::new("memory", &format!("projects/{name}.md")).map_err(|e| e.to_string())?;
        context.check_write(&page_url)?;
        context.check_write(&index_url)?;
        if server.web.locate(&page_url).is_ok() || server.web.locate(&memory_url).is_ok() {
            return Err(format!(
                "{page_url} already exists; open it, the project may exist"
            ));
        }

        let template = match args
            .template
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            None => None,
            Some(site) => {
                let site = site.trim_start_matches("site://").trim_end_matches('/');
                let site_url = SiteUrl::new(site, "").map_err(|e| e.to_string())?;
                context.check_read(&site_url)?;
                let dir = server.web.root().join(site).join("templates/project");
                if !dir.is_dir() {
                    return Err(format!("site://{site} has no templates/project/ folder"));
                }
                Some((site.to_string(), dir))
            }
        };

        let projects = expand_home(&server.config().projects.dir);
        let folder = projects.join(name);
        if folder.exists() {
            return Err(format!(
                "{} already exists; not touching it (set_project makes it active)",
                tilde(&folder)
            ));
        }
        std::fs::create_dir_all(&projects).map_err(|e| format!("{}: {e}", tilde(&projects)))?;
        // Fails if the folder appeared in the meantime, so nothing is overwritten.
        std::fs::create_dir(&folder).map_err(|e| format!("{}: {e}", tilde(&folder)))?;
        let filled = match &template {
            Some((_, dir)) => copy_template(dir, &folder, PLACEHOLDER, name),
            None => std::fs::write(folder.join("README.md"), format!("# {name}\n\n{about}\n")),
        };
        filled.map_err(|e| format!("filling {}: {e}", tilde(&folder)))?;
        // A new folder is this workspace's project, unless it lands inside another workspace's.
        if let Err(e) = server.use_project(&folder.display().to_string()) {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(e);
        }

        let path = tilde(&folder);
        let workflow = template.as_ref().map(|(site, _)| site.as_str());
        server
            .write_page(
                &page_url,
                &project_page(name, &about, &path, workflow),
                ChangedBy::Agent,
            )
            .map_err(|e| e.to_string())?;
        let registered = register(context, name, &about, &path, workflow);
        let from = workflow
            .map(|s| format!(" from site://{s}'s template"))
            .unwrap_or_default();
        let active = folder
            .canonicalize()
            .unwrap_or(folder)
            .display()
            .to_string();
        context
            .session
            .update_info(&server.hub, |info| info.project = Some(active));
        Ok(format!(
            "created {path}{from} and {page_url}{registered}.\n\
             It is now this session's active project."
        ))
    }
}

/// A folder name that stays inside the projects directory.
fn check_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "`{name}` is not a project name: use letters, digits, `-`, `_` or `.`, not starting with `.`"
        ))
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "/")
}

fn project_page(name: &str, about: &str, path: &str, workflow: Option<&str>) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d");
    let sites = workflow
        .map(|site| format!("sites: [{site}]\n"))
        .unwrap_or_else(|| "sites: []\n".to_string());
    format!(
        "---\ntitle: {name}\ntype: project\nroot: {path}\n{sites}\
         tags: [project]\nstatus: active\ncreated: {today}\n---\n# {name}\n\n{about}\n\nCreated {today}.\n"
    )
}

/// Lists the project in `site://projects`, Home and the memory registry. Returns a note.
fn register(
    context: &ToolContext,
    name: &str,
    about: &str,
    path: &str,
    workflow: Option<&str>,
) -> String {
    let server = &context.server;
    let mut notes: Vec<String> = Vec::new();

    let index_url = SiteUrl::new("projects", "").expect("valid");
    if let Ok(page) = server.web.read_page(&index_url) {
        let link = format!("[{name}]({name}.md)");
        let row = format!("| {link} | `{path}` | {} |", workflow.unwrap_or(""));
        let text = append_table_row(&page.raw, "Projects", &row).unwrap_or_else(|| {
            let table = format!("| project | root | sites |\n|---------|------|-------|\n{row}");
            add_section(&page.raw, "Projects", &table, "Projects")
        });
        match server.write_page(&index_url, &text, ChangedBy::Agent) {
            Ok(_) => notes.push("listed it in the projects index".into()),
            Err(e) => notes.push(format!("registering it in the projects index failed: {e}")),
        }
    }

    let home = SiteUrl::new("home", "").expect("valid");
    if let Ok(page) = server.web.read_page(&home)
        && !page.raw.contains("(site://projects)")
        && let Err(e) = crate::tools::add_to_home(
            server,
            "projects",
            "the things you work on: a folder, a page, and the sites that operate on it.",
            ChangedBy::Agent,
        )
    {
        notes.push(format!("adding the projects site to Home failed: {e}"));
    }

    let memory_url = SiteUrl::new("memory", "").expect("valid");
    if let Ok(page) = server.web.read_page(&memory_url) {
        let link = format!("[{name}](site://projects/{name})");
        let row = format!("| {link} | `{path}` | active | {about} |");
        let mut text = append_table_row(&page.raw, "Projects registry", &row).unwrap_or_else(|| {
            let table = format!(
                "| project | path | status | what it is |\n|---------|------|--------|------------|\n{row}"
            );
            add_section(&page.raw, "Projects registry", &table, "Pages")
        });
        if let Some(listed) = append_list_item(&text, "Pages", &link) {
            text = listed;
        }
        match server.write_page(&memory_url, &text, ChangedBy::Agent) {
            Ok(_) => notes.push("linked it from memory".into()),
            Err(e) => notes.push(format!("linking it from memory failed: {e}")),
        }
    }

    if notes.is_empty() {
        String::new()
    } else {
        format!(" and {}", notes.join(", "))
    }
}
