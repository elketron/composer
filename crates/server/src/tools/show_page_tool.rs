use async_trait::async_trait;
use serde_json::{Value, json};

use composer_protocol::DeskTab;

use crate::app_upstream::{AppUpstream, listed};
use crate::tools::stage_tools::put;
use crate::tools::{Tool, ToolContext};

/// Puts a page on the user's stage (ticket 0174).
pub struct ShowPageTool;

#[async_trait]
impl Tool for ShowPageTool {
    fn name(&self) -> &'static str {
        "show_page"
    }

    fn description(&self) -> &'static str {
        "Put a page on the user's stage, next to the chat (a tab on this session's desk), e.g. \
         the plan you are talking about. `app://<name>/<path>` shows one of the user's local apps \
         instead (`desk` lists them); its tab moves to that path if it is already open."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "site://plans/x, or app://canvas/" } },
            "required": ["url"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let raw = args["url"].as_str().unwrap_or_default();
        if let Some(app) = raw.trim().strip_prefix(APP_SCHEME) {
            return show_app(context, app);
        }
        let url = context.url(raw)?;
        context
            .server
            .web
            .locate(&url)
            .map_err(|e| format!("{e}. Use `search` to find pages."))?;
        let tab = put(
            context,
            DeskTab::Page {
                url: url.to_string(),
            },
        )?;
        Ok(format!("{tab} is on the stage"))
    }
}

/// How the agent names a local app: `app://<name>/<path>`.
const APP_SCHEME: &str = "app://";

/// `app://<name>/<path>` (the part after the scheme) as the app's tab, when it is configured.
fn show_app(context: &ToolContext, app: &str) -> Result<String, String> {
    let (name, path) = app.split_once('/').unwrap_or((app, ""));
    let config = context.server.config();
    if AppUpstream::find(&config, name).is_none() {
        let known = listed(&config);
        return Err(match known.is_empty() {
            true => format!("no app `{name}`: none is configured (`[apps.<name>]` in config.toml)"),
            false => format!(
                "no app `{name}`; the apps: {}",
                known
                    .iter()
                    .map(|a| format!("app://{}/", a.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    let tab = put(
        context,
        DeskTab::App {
            name: name.to_string(),
            path: path.to_string(),
        },
    )?;
    Ok(format!("{tab} is on the stage"))
}
