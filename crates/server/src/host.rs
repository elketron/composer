use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;
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
/// and the map of workspaces. Each workspace's API is mounted at `/w/<name>/`; the only route
/// outside them is `/api/workspaces`.
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

    /// `default`, else the first by name: whose config holds the host's own settings.
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
        let mut router = axum::Router::new().route("/api/workspaces", self.workspace_list()?);
        for server in self.workspaces.values() {
            let base = server.home.route_base();
            server.start_background(addr, &base)?;
            router = router.nest(&base, crate::api::router(server.clone()));
        }
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

    /// `GET /api/workspaces`: the workspaces the presented token opens, as `[{name, base}]`. The
    /// master token lists all of them; a workspace's (or its devices') token lists that one; any
    /// other gets 404, like everything else it can't open.
    fn workspace_list(&self) -> Result<axum::routing::MethodRouter> {
        let master = crate::init::read_or_create_token(&self.home.master_token_file())?;
        let workspaces: Arc<Vec<(WorkspaceName, Arc<Server>)>> = Arc::new(
            self.workspaces
                .iter()
                .map(|(name, server)| (name.clone(), server.clone()))
                .collect(),
        );
        Ok(get(move |headers: HeaderMap, uri: Uri| async move {
            let Some(token) = presented_token(&headers, &uri) else {
                return not_found();
            };
            let open: Vec<_> = workspaces
                .iter()
                .filter(|(_, server)| token == master || server.accepts_token(&token))
                .map(|(name, server)| json!({ "name": name.as_str(), "base": server.home.route_base() }))
                .collect();
            if open.is_empty() {
                return not_found();
            }
            Json(open).into_response()
        }))
    }
}

/// The token from `Authorization: Bearer` or `?token=`.
fn presented_token(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let query = uri
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v);
    bearer.or(query).map(str::to_string)
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}
