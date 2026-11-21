//! What a site's `mcp.toml` does when the site is set, and which of its servers a run uses.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::mcp_bundles::InstalledMcpBundles;
use crate::server::Server;
use crate::session::RunSink;
use crate::site_mcp::{
    ApprovedMcp, Note, SiteServers, approved_now, ensure_approved, load, resolve,
};

/// Setting `site` for `run`: asks the user about the site's `mcp.toml` when these bytes were
/// never approved (and the run has a session to ask in), then starts its servers (pooled: a
/// server some other site or session already runs is shared). What happened, as a note.
pub async fn enter(
    server: &Arc<Server>,
    run: &dyn RunSink,
    site: &str,
    cancel: &CancellationToken,
) -> Note {
    let Some(file) = load(&server.web, site) else {
        server.site_mcp.remove(site);
        return Note::default();
    };
    let session = run.session();
    let gate = ensure_approved(server, session.as_deref(), site, &file, cancel).await;
    let mut note = gate.note;
    let Some(approved) = gate.approved else {
        server.site_mcp.remove(site);
        return note;
    };
    let servers = activate(server, &approved);
    note.errors.extend(servers.errors.iter().cloned());
    note.errors.extend(server.mcp().connect(&servers).await);
    note
}

/// The servers of `site` a run in it uses: those of its `mcp.toml` when the user approved the
/// bytes that are there now, else none. Never asks: that is [`enter`]'s, when the site is set.
/// Servers connect at their first use.
pub fn active(server: &Server, site: &str) -> Option<Arc<SiteServers>> {
    let Some(file) = load(&server.web, site) else {
        server.site_mcp.remove(site);
        return None;
    };
    if let Some(current) = server.site_mcp.get(site)
        && current.hash == file.hash
    {
        return Some(current);
    }
    match approved_now(server, site, &file) {
        Some(approved) => Some(activate(server, &approved)),
        None => {
            server.site_mcp.remove(site);
            None
        }
    }
}

/// `approved` made into servers and kept for the site, unless the same bytes already are.
fn activate(server: &Server, approved: &ApprovedMcp) -> Arc<SiteServers> {
    if let Some(current) = server.site_mcp.get(approved.site())
        && current.hash == approved.hash()
    {
        return current;
    }
    let installed = InstalledMcpBundles::load(&server.home.mcps(), &server.home.mcps_state());
    let user_home = std::env::var("HOME").unwrap_or_default();
    let servers = Arc::new(resolve(
        approved,
        &server.config().mcp,
        &installed,
        &user_home,
    ));
    server.site_mcp.set(servers.clone());
    servers
}
