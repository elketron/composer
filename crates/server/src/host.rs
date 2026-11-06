use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::net::TcpListener;

use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::server::{Server, shutdown_signal};
use crate::workspace_name::WorkspaceName;

/// The process that serves every workspace of one composer home.
///
/// Each workspace runs as its own [`Server`]: its config, web, index, sessions, approvals,
/// workers and watchers belong to it alone, and every agent tool is handed that `Server`, so a
/// tool can only reach its own workspace. The host owns what is shared: the home, the listener,
/// and the map of workspaces. Each workspace's API is mounted at `/w/<name>/`.
pub struct Host {
    home: Home,
    workspaces: BTreeMap<WorkspaceName, Arc<Server>>,
}

impl Host {
    /// Opens every workspace of `home`. A home from before workspaces has to be migrated first.
    pub async fn start(home: Home) -> Result<Self> {
        if home.is_single_workspace() {
            return Err(ServerError::Invalid(format!(
                "{} is a single-workspace home from before workspaces: stop any running server, \
                 then run `composer migrate` to move it into workspaces/default/",
                home.root().display()
            )));
        }
        let names = home.workspace_names()?;
        if names.is_empty() {
            return Err(ServerError::Invalid(format!(
                "{} has no workspaces: run `composer init` first",
                home.root().display()
            )));
        }
        let mut workspaces = BTreeMap::new();
        for name in names {
            tracing::info!("workspace {name}");
            let server = Server::start(home.workspace(&name)).await?;
            workspaces.insert(name, server);
        }
        Ok(Self::new(home, workspaces))
    }

    /// A host over servers already started (each one's [`Server::home`] is its workspace).
    pub fn new(home: Home, workspaces: BTreeMap<WorkspaceName, Arc<Server>>) -> Self {
        assert!(
            !workspaces.is_empty(),
            "a host serves at least one workspace"
        );
        Self { home, workspaces }
    }

    pub fn home(&self) -> &Home {
        &self.home
    }

    pub fn workspace(&self, name: &WorkspaceName) -> Option<&Arc<Server>> {
        self.workspaces.get(name)
    }

    pub fn workspaces(&self) -> impl Iterator<Item = (&WorkspaceName, &Arc<Server>)> {
        self.workspaces.iter()
    }

    /// `default`, else the first by name: what the unprefixed routes serve until clients address
    /// workspaces themselves (ticket 0146).
    pub fn default_workspace(&self) -> &Arc<Server> {
        self.workspaces
            .get(&WorkspaceName::default_workspace())
            .or_else(|| self.workspaces.values().next())
            .expect("start refuses a home without workspaces")
    }

    /// `server.bind` (global config, or the default workspace's override).
    pub fn bind(&self) -> String {
        self.default_workspace().config().server.bind.clone()
    }

    /// Starts every workspace's background work and serves all their APIs on `listener` until it
    /// fails or the process is told to stop, then stops every workspace's workers.
    pub async fn serve(&self, listener: TcpListener) -> Result<()> {
        let addr = listener
            .local_addr()
            .map_err(|e| ServerError::Invalid(format!("listener address: {e}")))?;
        let mut router = axum::Router::new();
        for (name, server) in &self.workspaces {
            let base = format!("/w/{name}");
            server.start_background(addr, &base)?;
            router = router.nest(&base, crate::api::router(server.clone()));
        }
        // Transitional (ticket 0145 → 0146): the unprefixed routes still serve the default
        // workspace, so existing clients keep working until they address workspaces.
        let router = router.merge(crate::api::router(self.default_workspace().clone()));
        let serving = axum::serve(listener, router);
        tokio::select! {
            served = serving => {
                served.map_err(|e| ServerError::Invalid(format!("server stopped: {e}")))?;
            }
            _ = shutdown_signal() => {
                tracing::info!("shutting down");
            }
        }
        // Workers run in their own process groups: stop them, or they outlive the server.
        for server in self.workspaces.values() {
            server.workers.shutdown().await;
        }
        Ok(())
    }
}
