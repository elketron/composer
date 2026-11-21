//! Setting a run's site: the one implementation behind the `set_site` tool and the user's own
//! switch for a session (a client command).

use std::sync::Arc;

use composer_protocol::ServerMessage;
use tokio_util::sync::CancellationToken;

use crate::agent::{Entered, Site, SiteSet};
use crate::server::Server;
use crate::session::RunSink;

/// Puts the run in the site `name`, or (`None`, or blank) out of the one it is in. An error
/// when the site does not exist; then nothing changes.
///
/// Entering asks the user about the site's `mcp.toml` when its bytes were never approved
/// (waiting for the answer until `cancel`), then starts its servers. The run's MCP clients hear
/// `tools.changed` when the site is set and again once its servers are up.
pub async fn set_site(
    server: &Arc<Server>,
    run: &dyn RunSink,
    name: Option<&str>,
    cancel: &CancellationToken,
) -> Result<SiteSet, String> {
    let tools_changed = || {
        server.hub.send(ServerMessage::ToolsChanged {
            session: run.id().to_string(),
        })
    };
    let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else {
        let previous = run.site();
        run.set_site(None);
        return Ok(match previous {
            Some(site) => {
                tools_changed();
                SiteSet::Left(site)
            }
            None => SiteSet::NotInSite,
        });
    };
    let site = Site::load(&server.web, name)?;
    run.set_site(Some(site.name.clone()));
    tools_changed();
    let mcp = crate::site_mcp::enter(server, run, &site.name, cancel).await;
    if mcp.approved {
        tools_changed();
    }
    Ok(SiteSet::Entered(Entered {
        mcp,
        site: site.name,
        title: site.title,
    }))
}
