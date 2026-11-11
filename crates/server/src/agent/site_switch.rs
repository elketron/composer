//! Entering and leaving a site for a session: the one implementation behind the `enter_site` and
//! `leave_site` tools and the user's own switch (a client command).
//!
//! Two entry points make the one difference explicit: an agent may enter only sites its page's
//! `sites:` lets it read ([`enter_site_as_agent`]); the user is not an agent and has no such
//! scope ([`enter_site_as_client`]).

use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use composer_protocol::{DeskBy, DeskTab, ServerMessage};

use crate::agent::{AgentDefinition, AppLaunch, Entered, Site};
use crate::config_sections::ApplicationConfig;
use crate::server::Server;
use crate::session::Session;
use crate::viewer_upstream::ViewerUpstream;

/// An agent enters `name`: refused unless its `sites:` lets it read the site.
pub async fn enter_site_as_agent(
    server: &Arc<Server>,
    session: &Session,
    agent: &AgentDefinition,
    name: &str,
) -> Result<Entered, String> {
    let site = Site::load(&server.web, name)?;
    if !agent.sites.can_read(&site.name) {
        return Err(format!(
            "agent `{}` may not read site `{}`",
            agent.name, site.name
        ));
    }
    enter(server, session, site, DeskBy::Agent).await
}

/// The user enters `name` (a client command): no agent scope applies.
pub async fn enter_site_as_client(
    server: &Arc<Server>,
    session: &Session,
    name: &str,
) -> Result<Entered, String> {
    let site = Site::load(&server.web, name)?;
    enter(server, session, site, DeskBy::User).await
}

/// Leaves the site the session is in; returns its name, `None` when it was in none. Does not
/// close the site's application.
pub fn leave_site(server: &Server, session: &Session) -> Option<String> {
    let previous = session.info().site;
    session.update_info(&server.hub, |info| info.site = None);
    previous
}

async fn enter(
    server: &Arc<Server>,
    session: &Session,
    site: Site,
    by: DeskBy,
) -> Result<Entered, String> {
    if site.needs_project && session.info().project.is_none() {
        return Err(format!(
            "site://{} needs an active project; set one with `set_project` first",
            site.name
        ));
    }
    session.update_info(&server.hub, |info| info.site = Some(site.name.clone()));
    let app = launch_app(server, session, &site).await;
    let running = matches!(
        app,
        AppLaunch::Launched { .. } | AppLaunch::AlreadyRunning { .. }
    );
    let live_view = running && open_live_view(server, session, by);
    let mcp_refused = server.mcp().refused_grants(&site);
    Ok(Entered {
        mcp_refused,
        site: site.name,
        title: site.title,
        app,
        live_view,
    })
}

/// Puts the live view on the session's desk when the workspace has a viewer, and tells its
/// clients. The user's entry focuses it. The agent's does what every agent open does (see
/// `tools/stage_tools.rs`): the tab arrives unfocused and `unseen`, and `desk.changed` names it
/// (`opened`), so a client following the agent shows it and one that is not badges it. Never an
/// error: entering does not depend on the desk. Whether it was opened.
fn open_live_view(server: &Server, session: &Session, by: DeskBy) -> bool {
    if !matches!(ViewerUpstream::from_config(&server.config()), Ok(Some(_))) {
        return false;
    }
    let focus = by == DeskBy::User;
    let mut opened = String::new();
    let desk = session.change_desk(|desk| {
        opened = desk.open(DeskTab::LiveView, focus, by)?;
        Ok(())
    });
    let Ok(desk) = desk else {
        return false;
    };
    server.hub.send(ServerMessage::DeskChanged {
        session: session.id().to_string(),
        desk,
        by,
        opened: (by == DeskBy::Agent).then_some(opened),
    });
    true
}

/// Starts the site's `app:` through the `launch` tool of the MCP server its
/// `[applications.<app>]` entry names. This is the server's own call: no agent `tools:` check and
/// no approval, because the command comes from `config.toml`, never from the page (agents can
/// write pages). Never an error: entering does not depend on the application.
async fn launch_app(server: &Server, session: &Session, site: &Site) -> AppLaunch {
    let Some(app) = site.app.clone() else {
        return AppLaunch::NoApp;
    };
    let skipped = |reason: String| AppLaunch::Skipped {
        app: app.clone(),
        reason,
    };
    let config = server.config();
    let Some(entry) = config.applications.get(&app) else {
        return skipped(format!(
            "there is no [applications.{app}] in config.toml; add one with a `command` to start it"
        ));
    };
    if server.parent.is_some() {
        return skipped(
            "a worker process cannot start applications; enter the site from the session itself"
                .into(),
        );
    }
    let mcp = server.mcp();
    let launcher = &entry.mcp;
    if running(&mcp, entry).await {
        return AppLaunch::AlreadyRunning { app };
    }
    let project = session.info().project;
    match mcp
        .call_internal(launcher, "launch", launch_args(entry, project.as_deref()))
        .await
    {
        Ok(result) if !result.is_error => AppLaunch::Launched { app },
        Ok(result) => skipped(format!(
            "the `launch` tool of MCP server `{launcher}` failed: {}",
            result.text.trim()
        )),
        Err(e) => skipped(format!(
            "MCP server `{launcher}` could not launch it ({e}); check [mcp.{launcher}] in \
             config.toml, or start it by hand"
        )),
    }
}

fn launch_args(entry: &ApplicationConfig, project: Option<&str>) -> serde_json::Value {
    let mut args = json!({ "command": entry.command, "args": entry.args, "env": entry.env });
    if let Some(project) = project {
        args["cwd"] = json!(project);
    }
    args
}

/// Whether the launching server's `list_apps` output has a line mentioning the application. The
/// output format of `list_apps` is not specified, so this is a heuristic: a line that contains
/// the command's name (its file name, when the command is a path). If `list_apps` fails or is
/// missing, the application is taken as not running and the launch goes ahead.
async fn running(mcp: &crate::mcp_clients::McpServers, entry: &ApplicationConfig) -> bool {
    let Ok(listing) = mcp.call_internal(&entry.mcp, "list_apps", json!({})).await else {
        return false;
    };
    if listing.is_error {
        return false;
    }
    let needle = Path::new(&entry.command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&entry.command);
    !needle.is_empty() && listing.text.lines().any(|line| line.contains(needle))
}
