//! The agent puts things on the stage (plan phase 5, ticket 0174): an app, a page, the changes,
//! as tabs on its session's desk. The server never moves the shown tab for the agent: what it
//! opens is `unseen`, and `desk.changed` names it (`opened`), so a client following the agent
//! shows it and one not following badges it.

use async_trait::async_trait;
use composer_protocol::{Desk, DeskBy, DeskTab, ServerMessage};
use serde_json::{Value, json};

use crate::apps::{AttachReason, attached_apps, find_project_by, home_apps};
use crate::tools::{Tool, ToolContext};

/// Opens `tab` on the session's desk for the agent (unfocused, unseen) and tells its clients.
fn put(context: &ToolContext, tab: DeskTab) -> Result<String, String> {
    let mut opened = String::new();
    let desk = context.session.change_desk(|desk| {
        opened = desk.open(tab, false, DeskBy::Agent)?;
        Ok(())
    })?;
    point(context, desk, &opened);
    Ok(opened)
}

fn point(context: &ToolContext, desk: Desk, tab: &str) {
    context.server.hub.send(ServerMessage::DeskChanged {
        session: context.session.id().to_string(),
        desk,
        by: DeskBy::Agent,
        opened: Some(tab.to_string()),
    });
}

/// The session's project, by name.
fn session_project(context: &ToolContext) -> Option<String> {
    context.session.info().project.and_then(|folder| {
        find_project_by(&context.server, &folder)
            .ok()
            .map(|(name, _)| name.to_string())
    })
}

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

pub struct ShowPageTool;

#[async_trait]
impl Tool for ShowPageTool {
    fn name(&self) -> &'static str {
        "show_page"
    }

    fn description(&self) -> &'static str {
        "Put a page on the user's stage, next to the chat (a tab on this session's desk), e.g. \
         the plan you are talking about."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "site://plans/x" } },
            "required": ["url"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let raw = args["url"].as_str().unwrap_or_default();
        let url = context.url(raw)?;
        context.check_read(&url)?;
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

pub struct DeskTool;

#[async_trait]
impl Tool for DeskTool {
    fn name(&self) -> &'static str {
        "desk"
    }

    fn description(&self) -> &'static str {
        "What is on the user's stage for this session (its tabs, the one shown, the ones the user \
         hasn't looked at yet). `changes: true` puts the session project's changes on it; `point` \
         (a tab id) asks the user's client to show that tab."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "changes": { "type": "boolean", "description": "put the project's changes on the stage" },
                "point": { "type": "string", "description": "a tab id to show, e.g. app:cad@game" }
            }
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        if args["changes"].as_bool().unwrap_or(false) {
            let project = session_project(context)
                .ok_or("this session has no project: set one first (`set_project`)")?;
            let tab = put(context, DeskTab::Changes { project })?;
            return Ok(format!("{tab} is on the stage"));
        }
        let desk = context.session.desk();
        if let Some(tab) = args["point"].as_str() {
            if !desk.tabs.iter().any(|t| t.id == tab) {
                return Err(format!(
                    "no tab `{tab}` on the desk (call `desk` to list them)"
                ));
            }
            point(context, desk, tab);
            return Ok(format!("pointed the user at {tab}"));
        }
        if desk.tabs.is_empty() {
            return Ok("The stage is empty (the project home).".into());
        }
        let lines: Vec<String> = desk
            .tabs
            .iter()
            .map(|t| {
                let shown = desk.active.as_deref() == Some(t.id.as_str());
                let mark = match (shown, t.unseen) {
                    (true, _) => " (shown)",
                    (false, true) => " (not looked at yet)",
                    _ => "",
                };
                format!("- {}{mark}", t.id)
            })
            .collect();
        Ok(lines.join("\n"))
    }
}
