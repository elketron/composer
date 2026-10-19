use async_trait::async_trait;
use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::tools::args::NewSiteArgs;
use crate::tools::markdown_sections::append_table_row;
use crate::tools::{Tool, ToolContext};

pub struct NewSiteTool;

#[async_trait]
impl Tool for NewSiteTool {
    fn name(&self) -> &'static str {
        "new_site"
    }

    fn description(&self) -> &'static str {
        "Create a new site (a top-level folder of the web) with its index.md homepage and list it \
         on Home. The homepage gets stub Layout, Page format, Rules and Pages sections; fill them \
         in with `edit` for what the site will hold. Then the site can take actions (write \
         site://<name>/justfile) and an app (`new_app`)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "folder name: letters, digits, - or _, e.g. calendar" },
                "title": { "type": "string", "description": "shown in lists; defaults to the name" },
                "about": { "type": "string", "description": "one line: what the site is for" }
            },
            "required": ["name", "about"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: NewSiteArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let name = args
            .name
            .trim()
            .trim_start_matches("site://")
            .trim_end_matches('/');
        let url = SiteUrl::new(name, "").map_err(|e| e.to_string())?;
        context.check_write(&url)?;
        let server = &context.server;
        let dir = server.workspace.root().join(name);
        if dir.exists() {
            return Err(format!("site://{name} already exists"));
        }
        let about = one_line(&args.about);
        let title = args
            .title
            .as_deref()
            .map(one_line)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| name.to_string());

        // `create_dir` fails if it appeared in the meantime; pages can only go into existing sites.
        std::fs::create_dir(&dir).map_err(|e| format!("creating site://{name}: {e}"))?;
        server
            .write_page(&url, &homepage(&title, &about), ChangedBy::Agent)
            .map_err(|e| e.to_string())?;
        let listed = list_on_home(context, name, &about);
        Ok(format!(
            "created site://{name} (index.md){listed}.\n\
             Next: fill in its Layout, Page format and Rules with `edit`, then write its pages."
        ))
    }
}

/// Model text for a table cell or frontmatter: one line, no pipes.
fn one_line(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "/")
}

fn homepage(title: &str, about: &str) -> String {
    let title_yaml = serde_json::to_string(title).expect("a string serializes");
    format!(
        "---\ntitle: {title_yaml}\ntype: index\n---\n# {title}\n\n{about}\n\n\
         ## Layout\n\n\
         | path | holds |\n\
         |------|-------|\n\
         | `index.md` | this page: what the site is for and its conventions |\n\n\
         ## Page format\n\n\
         ```markdown\n---\ntitle: Example\ntype: page\ntags: []\nupdated: YYYY-MM-DD\n---\n# Example\n```\n\n\
         ## Rules\n\n\
         - Keep the [pages](#pages) list below up to date when you add or remove a page.\n\n\
         ## Pages\n"
    )
}

/// Adds the site to the table on Home. Returns a note for the result.
fn list_on_home(context: &ToolContext, name: &str, about: &str) -> String {
    let home = SiteUrl::new("home", "").expect("valid");
    let not_listed = |why: &str| format!("; not listed on Home ({why}), add it there yourself");
    if context.check_write(&home).is_err() {
        return not_listed("you may not edit it");
    }
    let server = &context.server;
    let Ok(page) = server.workspace.read_page(&home) else {
        return not_listed("there is no site://home");
    };
    let row = format!("| [{name}](site://{name}) | {about} |");
    let Some(text) = append_table_row(&page.raw, "Sites", &row) else {
        return not_listed("it has no Sites table");
    };
    match server.write_page(&home, &text, ChangedBy::Agent) {
        Ok(_) => " and listed it on Home".into(),
        Err(e) => not_listed(&e.to_string()),
    }
}
