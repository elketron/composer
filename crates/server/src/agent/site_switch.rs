//! Setting a run's site: the one implementation behind the `set_site` tool and the user's own
//! switch for a session (a client command).

use std::sync::Arc;

use crate::agent::{Entered, Site, SiteSet};
use crate::server::Server;
use crate::session::RunSink;

/// Puts the run in the site `name`, or (`None`, or blank) out of the one it is in. An error
/// when the site does not exist; then nothing changes.
pub async fn set_site(
    server: &Arc<Server>,
    run: &dyn RunSink,
    name: Option<&str>,
) -> Result<SiteSet, String> {
    let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else {
        let previous = run.site();
        run.set_site(None);
        return Ok(match previous {
            Some(site) => SiteSet::Left(site),
            None => SiteSet::NotInSite,
        });
    };
    let site = Site::load(&server.web, name)?;
    run.set_site(Some(site.name.clone()));
    let mcp_refused = server.mcp().refused_grants(&site);
    Ok(SiteSet::Entered(Entered {
        mcp_refused,
        site: site.name,
        title: site.title,
    }))
}
