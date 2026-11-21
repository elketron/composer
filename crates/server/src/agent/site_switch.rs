//! Setting a run's site: the one implementation behind the `set_site` tool and the user's own
//! switch for a session (a client command).

use std::sync::Arc;

use composer_protocol::ServerMessage;
use tokio_util::sync::CancellationToken;

use crate::agent::{Entered, FolderNote, Site, SiteSet, ensure_folder_approved};
use crate::server::Server;
use crate::session::RunSink;
use crate::site_mcp::Note;

/// Puts the run in the site `name`, or (`None`, or blank) out of the one it is in. An error
/// when the site does not exist; then nothing changes.
///
/// Entering asks the user about the site's `folder:` and its `mcp.toml` when they were never
/// approved (waiting for the answers until `cancel`), then starts its servers ([`enter_site`]). The run's MCP clients hear
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
    let (folder, mcp) = enter_site(server, run, &site, cancel).await;
    if mcp.approved || folder.approved {
        tools_changed();
    }
    Ok(SiteSet::Entered(Entered {
        folder,
        mcp,
        site: site.name,
        title: site.title,
    }))
}

/// What entering `site` asks and starts, for `run`: its `folder:` (asked of the user's clients
/// when that folder was never approved for the site, and the run has a session to ask in), then
/// its `mcp.toml` (the same, then its servers start).
pub async fn enter_site(
    server: &Arc<Server>,
    run: &dyn RunSink,
    site: &Site,
    cancel: &CancellationToken,
) -> (FolderNote, Note) {
    let session = run.session();
    let folder = ensure_folder_approved(server, session.as_deref(), site, cancel).await;
    let mcp = crate::site_mcp::enter(server, run, &site.name, cancel).await;
    (folder, mcp)
}
