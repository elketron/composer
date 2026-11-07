//! The agent puts things on the stage (plan phase 5, ticket 0174): an app, a page, the changes,
//! as tabs on its session's desk. The server never moves the shown tab for the agent: what it
//! opens is `unseen`, and `desk.changed` names it (`opened`), so a client following the agent
//! shows it and one not following badges it.

use composer_protocol::{Desk, DeskBy, DeskTab, ServerMessage};

use crate::apps::find_project_by;
use crate::tools::ToolContext;

/// Opens `tab` on the session's desk for the agent (unfocused, unseen) and tells its clients.
pub(crate) fn put(context: &ToolContext, tab: DeskTab) -> Result<String, String> {
    let mut opened = String::new();
    let desk = context.session.change_desk(|desk| {
        opened = desk.open(tab, false, DeskBy::Agent)?;
        Ok(())
    })?;
    point(context, desk, &opened);
    Ok(opened)
}

pub(crate) fn point(context: &ToolContext, desk: Desk, tab: &str) {
    context.server.hub.send(ServerMessage::DeskChanged {
        session: context.session.id().to_string(),
        desk,
        by: DeskBy::Agent,
        opened: Some(tab.to_string()),
    });
}

/// The session's project, by name.
pub(crate) fn session_project(context: &ToolContext) -> Option<String> {
    context.session.info().project.and_then(|folder| {
        find_project_by(&context.server, &folder)
            .ok()
            .map(|(name, _)| name.to_string())
    })
}
