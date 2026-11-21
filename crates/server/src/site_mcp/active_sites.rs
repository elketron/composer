//! The sites whose servers are in use: each with the servers of the bytes the user approved.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::site_mcp::SiteServers;

/// The approved servers of each site that was set (or used) since the start, by site. Shared by
/// every session in the site; the processes themselves are pooled by [`crate::mcp_clients::McpServers`].
#[derive(Debug, Default)]
pub struct ActiveSites {
    sites: Mutex<BTreeMap<String, Arc<SiteServers>>>,
}

impl ActiveSites {
    pub fn get(&self, site: &str) -> Option<Arc<SiteServers>> {
        self.sites.lock().expect("sites lock").get(site).cloned()
    }

    pub fn all(&self) -> Vec<Arc<SiteServers>> {
        self.sites
            .lock()
            .expect("sites lock")
            .values()
            .cloned()
            .collect()
    }

    pub(super) fn set(&self, servers: Arc<SiteServers>) {
        self.sites
            .lock()
            .expect("sites lock")
            .insert(servers.site.clone(), servers);
    }

    pub(super) fn remove(&self, site: &str) {
        self.sites.lock().expect("sites lock").remove(site);
    }

    /// Forgets every site: `config.toml` changed, and what a site's servers are made of
    /// (bundle settings, the names it may not take) with it. They are made again on next use.
    pub fn clear(&self) {
        self.sites.lock().expect("sites lock").clear();
    }
}
