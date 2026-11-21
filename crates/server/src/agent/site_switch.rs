//! Setting a session's site: the one implementation behind the `set_site` tool and the user's
//! own switch (a client command).

use std::sync::Arc;

use crate::agent::{Entered, Site, SiteSet};
use crate::server::Server;
use crate::session::Session;

/// Puts the session in the site `name`, or (`None`, or blank) out of the one it is in. An error
/// when the site does not exist; then nothing changes.
pub async fn set_site(
    server: &Arc<Server>,
    session: &Session,
    name: Option<&str>,
) -> Result<SiteSet, String> {
    let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else {
        let previous = session.info().site;
        session.update_info(&server.hub, |info| info.site = None);
        return Ok(match previous {
            Some(site) => SiteSet::Left(site),
            None => SiteSet::NotInSite,
        });
    };
    let site = Site::load(&server.web, name)?;
    session.update_info(&server.hub, |info| info.site = Some(site.name.clone()));
    let mcp_refused = server.mcp().refused_grants(&site);
    Ok(SiteSet::Entered(Entered {
        mcp_refused,
        site: site.name,
        title: site.title,
    }))
}
