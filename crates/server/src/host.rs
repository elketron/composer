use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::net::TcpListener;

use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::host_state::{HostState, Starter};
use crate::server::{Server, shutdown_signal};
use crate::workspace_name::WorkspaceName;

/// The process that serves every workspace of one composer home.
///
/// Each workspace runs as its own [`Server`]: its config, web, index, sessions, approvals,
/// workers and watchers belong to it alone, and every agent tool is handed that `Server`, so a
/// tool can only reach its own workspace. The host owns what is shared: the home, the listener,
/// and the map of workspaces. Each workspace's API is mounted at `/w/<name>/`; the only route
/// outside them is `/api/workspaces`.
pub struct Host {
    state: Arc<HostState>,
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
        Self::new(home, workspaces)
    }

    /// A host over servers already started (each one's [`Server::home`] is its workspace). They
    /// share one project ownership index (`state/projects.json`), and each workspace claims the
    /// projects its config names (`[projects."<path>"]`): two workspaces naming the same project,
    /// or nested ones, is an error that names both.
    pub fn new(home: Home, workspaces: BTreeMap<WorkspaceName, Arc<Server>>) -> Result<Self> {
        if workspaces.is_empty() {
            return Err(ServerError::Invalid(
                "a host serves at least one workspace".into(),
            ));
        }
        let state = Arc::new(HostState::new(home));
        for (name, server) in workspaces {
            state.add(name, server)?;
        }
        Ok(Self { state })
    }

    pub fn home(&self) -> &Home {
        &self.state.home
    }

    pub fn workspace(&self, name: &WorkspaceName) -> Option<Arc<Server>> {
        self.state.workspace(name)
    }

    pub fn workspaces(&self) -> Vec<(WorkspaceName, Arc<Server>)> {
        self.state.list()
    }

    /// `default`, else the first by name: whose config holds the host's own settings.
    pub fn default_workspace(&self) -> Arc<Server> {
        self.state
            .workspace(&WorkspaceName::default_workspace())
            .or_else(|| self.state.list().into_iter().next().map(|(_, s)| s))
            .expect("a host serves at least one workspace")
    }

    /// `server.bind` (global config, or the default workspace's override).
    pub fn bind(&self) -> String {
        self.default_workspace().config().server.bind.clone()
    }

    /// Creates workspace `name` (from `template`, if given) and serves it right away.
    pub async fn create(
        &self,
        name: &WorkspaceName,
        template: Option<&str>,
    ) -> Result<Arc<Server>> {
        self.state.create(name, template).await
    }

    /// How [`Host::create`] starts a new workspace's server (tests: with a mock provider).
    pub fn set_starter(&self, starter: Starter) {
        self.state.set_starter(starter);
    }

    /// Starts every workspace's background work and serves all their APIs on `listener` until it
    /// fails or the process is told to stop, then stops every workspace's workers. A workspace
    /// created meanwhile is served from then on.
    pub async fn serve(&self, listener: TcpListener) -> Result<()> {
        let addr = listener
            .local_addr()
            .map_err(|e| ServerError::Invalid(format!("listener address: {e}")))?;
        let master = crate::init::read_or_create_token(&self.state.home.master_token_file())?;
        self.state.listening(addr)?;
        let dispatching = self.state.clone();
        let router = axum::Router::new()
            .route(
                "/api/workspaces",
                crate::host_routes::workspaces(self.state.clone(), master),
            )
            .fallback(move |request| crate::host_routes::dispatch(dispatching.clone(), request));
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
        for (_, server) in self.state.list() {
            server.workers.shutdown().await;
        }
        Ok(())
    }
}
