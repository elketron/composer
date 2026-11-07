use async_trait::async_trait;
use serde_json::{Value, json};

use composer_protocol::DeskTab;

use crate::apps::{AttachReason, attached_apps, home_apps};
use crate::tools::stage_tools::{put, session_project};
use crate::tools::{Tool, ToolContext};

/// Puts an app on the user's stage (ticket 0174).
pub struct ShowAppTool;

#[async_trait]
impl Tool for ShowAppTool {
    fn name(&self) -> &'static str {
        "show_app"
    }

    fn description(&self) -> &'static str {
        "Put an app on the user's stage, next to the chat, so you look at the same thing: a tab on \
         this session's desk (the user's client shows it if it follows you, else badges it). On \
         the session's project by default; `project` names another of this workspace's projects, \
         `home: true` opens it on Home. The app must be one of the project's apps (`when` or a \
         pin)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "site": { "type": "string", "description": "the app's site, e.g. cad" },
                "project": { "type": "string", "description": "a project's name (default: this session's)" },
                "home": { "type": "boolean", "description": "open it on Home, not on a project" }
            },
            "required": ["site"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let site = args["site"].as_str().unwrap_or_default().to_string();
        let home = args["home"].as_bool().unwrap_or(false);
        let project = match (home, args["project"].as_str()) {
            (true, _) => None,
            (false, Some(name)) => Some(name.to_string()),
            (false, None) => session_project(context),
        };
        let server = context.server.clone();
        let (check_site, check_project) = (site.clone(), project.clone());
        tokio::task::spawn_blocking(move || match &check_project {
            Some(name) => {
                let (_, _, apps, _) = attached_apps(&server, name)?;
                match apps.iter().find(|a| a.site == check_site) {
                    Some(app) if app.why == AttachReason::LocalNotAllowed => Err(format!(
                        "`{check_site}` is a local app of `{name}` the user hasn't allowed (they \
                         run `composer app allow {name} {check_site}`)"
                    )),
                    Some(_) => Ok(()),
                    None => Err(format!(
                        "site://{check_site} isn't one of project `{name}`'s apps: give its `app:` \
                         block a `when` that matches, or ask the user to pin it (`apps = \
                         [\"{check_site}\"]` in the project's [projects] entry)"
                    )),
                }
            }
            None => match home_apps(&server).apps.iter().any(|a| a.site == check_site) {
                true => Ok(()),
                false => Err(format!(
                    "site://{check_site} isn't a Home app (`scope: global` or `both`)"
                )),
            },
        })
        .await
        .map_err(|e| e.to_string())??;
        let place = project
            .as_deref()
            .map_or("Home".to_string(), |p| format!("project `{p}`"));
        let tab = put(context, DeskTab::App { site, project })?;
        Ok(format!("{tab} is on the stage ({place})"))
    }
}
