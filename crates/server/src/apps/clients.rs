use crate::server::Server;

/// `site`'s app is a client of the workspace: listed by the user in `[apps] clients`
/// (ticket 0170). What the site's own manifest says doesn't make it one.
pub fn is_client(server: &Server, site: &str) -> bool {
    server.config().apps.clients.iter().any(|c| c == site)
}
