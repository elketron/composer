use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::NewAppArgs;
use crate::tools::{Tool, ToolContext};

/// Replaced by the site name in every scaffold file.
const PLACEHOLDER: &str = "__SITE__";

pub struct NewAppTool;

#[async_trait]
impl Tool for NewAppTool {
    fn name(&self) -> &'static str {
        "new_app"
    }

    fn description(&self) -> &'static str {
        "Give a site a human-facing app: copies the Svelte app template into <site>/ui and adds a \
         `ui-build` action to the site's justfile. Then run `ui-build` (asks the user's approval the \
         first time); the user opens the app from the TUI with `a`. Edit ui/src/App.svelte to build \
         the app; it imports theme, components and the server client from `$sdk`. App state the \
         agent should see belongs in the site's files (putPage/putFile), not inside the app."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "site": { "type": "string", "description": "an existing site, e.g. design" } },
            "required": ["site"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: NewAppArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let site = args
            .site
            .trim_start_matches("site://")
            .trim_end_matches('/')
            .to_string();
        context.check_write(&context.url(&format!("site://{site}"))?)?;
        let server = &context.server;
        let site_dir = server.workspace.root().join(&site);
        if !site_dir.is_dir() {
            return Err(format!(
                "site://{site} does not exist; write its index.md first"
            ));
        }
        let ui = site_dir.join("ui");
        if ui.exists() {
            return Err(format!("site://{site} already has an app in ui/"));
        }
        let template = server.home.templates().join("app");
        if !template.is_dir() {
            return Err("the app template is missing; run `composer init`".into());
        }

        copy_template(&template, &ui, &site).map_err(|e| format!("copying the template: {e}"))?;

        let justfile = site_dir.join("justfile");
        let existing = std::fs::read_to_string(&justfile).unwrap_or_default();
        if !existing.lines().any(|l| l.starts_with("ui-build")) {
            let separator = if existing.is_empty() || existing.ends_with("\n\n") {
                ""
            } else if existing.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            let recipe = format!(
                "{separator}# Build this site's app (ui/dist), served at /{site}/.\nui-build:\n    cd ui && bun install && bun run build\n"
            );
            std::fs::write(&justfile, format!("{existing}{recipe}")).map_err(|e| e.to_string())?;
        }
        Ok(format!(
            "created site://{site}/ui (Svelte + $sdk) and the `ui-build` action.\n\
             Next: edit ui/src/App.svelte for what the site needs, then `run {site} ui-build`. \
             The user opens it with `a` in the TUI (or :app {site})."
        ))
    }
}

/// Copies the scaffold, replacing the placeholder in text files. Skips build output.
fn copy_template(from: &Path, to: &Path, site: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "node_modules" || name == "dist" {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_template(&entry.path(), &target, site)?;
            continue;
        }
        let bytes = std::fs::read(entry.path())?;
        match String::from_utf8(bytes) {
            Ok(text) => std::fs::write(&target, text.replace(PLACEHOLDER, site))?,
            Err(binary) => std::fs::write(&target, binary.into_bytes())?,
        }
    }
    Ok(())
}
