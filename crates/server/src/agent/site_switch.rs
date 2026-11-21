//! Entering and leaving a site for a session: the one implementation behind the `enter_site` and
//! `leave_site` tools and the user's own switch (a client command).
//!
//! Two entry points make the one difference explicit: an agent may enter only sites its page's
//! `sites:` lets it read ([`enter_site_as_agent`]); the user is not an agent and has no such
//! scope ([`enter_site_as_client`]).

use std::sync::Arc;

use crate::agent::{AgentDefinition, Entered, Site};
use crate::server::Server;
use crate::session::Session;

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
    enter(server, session, site).await
}

/// The user enters `name` (a client command): no agent scope applies.
pub async fn enter_site_as_client(
    server: &Arc<Server>,
    session: &Session,
    name: &str,
) -> Result<Entered, String> {
    let site = Site::load(&server.web, name)?;
    enter(server, session, site).await
}

/// Leaves the site the session is in; returns its name, `None` when it was in none.
pub fn leave_site(server: &Server, session: &Session) -> Option<String> {
    let previous = session.info().site;
    session.update_info(&server.hub, |info| info.site = None);
    previous
}

async fn enter(server: &Arc<Server>, session: &Session, site: Site) -> Result<Entered, String> {
    if site.needs_project && session.info().project.is_none() {
        return Err(format!(
            "site://{} needs an active project; start the session in one",
            site.name
        ));
    }
    session.update_info(&server.hub, |info| info.site = Some(site.name.clone()));
    let mcp_refused = server.mcp().refused_grants(&site);
    Ok(Entered {
        mcp_refused,
        site: site.name,
        title: site.title,
    })
}
