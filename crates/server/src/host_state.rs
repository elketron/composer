use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, OnceLock, RwLock};

use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::project::{ProjectOwnership, ProjectPath};
use crate::server::Server;
use crate::template_name::TemplateName;
use crate::workspace_create::create_workspace;
use crate::workspace_home::WorkspaceHome;
use crate::workspace_name::WorkspaceName;

/// How a new workspace's [`Server`] is started: [`Server::start`] (connects the configured
/// provider), or something else in tests.
pub type Starter = Arc<
    dyn Fn(WorkspaceHome) -> Pin<Box<dyn Future<Output = Result<Arc<Server>>> + Send>>
        + Send
        + Sync,
>;

/// What the host's routes share: the home, its workspaces (which can grow while serving) with
/// their routers, and the project ownership index they all join.
pub struct HostState {
    pub home: Home,
    ownership: Arc<ProjectOwnership>,
    workspaces: RwLock<BTreeMap<WorkspaceName, Arc<Server>>>,
    routes: RwLock<BTreeMap<WorkspaceName, axum::Router>>,
    listen: OnceLock<SocketAddr>,
    starter: RwLock<Starter>,
}

impl HostState {
    pub fn new(home: Home) -> Self {
        let ownership = Arc::new(ProjectOwnership::load(home.projects_file()));
        let starter: Starter = Arc::new(|workspace| Box::pin(Server::start(workspace)));
        Self {
            home,
            ownership,
            workspaces: RwLock::new(BTreeMap::new()),
            routes: RwLock::new(BTreeMap::new()),
            listen: OnceLock::new(),
            starter: RwLock::new(starter),
        }
    }

    pub fn set_starter(&self, starter: Starter) {
        *self.starter.write().expect("starter lock") = starter;
    }

    pub fn workspace(&self, name: &WorkspaceName) -> Option<Arc<Server>> {
        self.workspaces
            .read()
            .expect("workspaces lock")
            .get(name)
            .cloned()
    }

    pub fn list(&self) -> Vec<(WorkspaceName, Arc<Server>)> {
        self.workspaces
            .read()
            .expect("workspaces lock")
            .iter()
            .map(|(name, server)| (name.clone(), server.clone()))
            .collect()
    }

    /// The router of workspace `name`, once the host is listening.
    pub fn route(&self, name: &str) -> Option<axum::Router> {
        let name = WorkspaceName::new(name).ok()?;
        self.routes.read().expect("routes lock").get(&name).cloned()
    }

    /// Adds a started workspace: it joins the ownership index and claims the projects its config
    /// names; when the host is already listening, its background work starts and its routes
    /// answer from now on.
    pub fn add(&self, name: WorkspaceName, server: Arc<Server>) -> Result<()> {
        if self.workspace(&name).is_some() {
            return Err(ServerError::Invalid(format!(
                "the host already serves workspace {name}"
            )));
        }
        server.share_ownership(self.ownership.clone())?;
        claim_configured_projects(&name, &server)?;
        if let Some(addr) = self.listen.get() {
            self.serve_one(*addr, &name, &server)?;
        }
        self.workspaces
            .write()
            .expect("workspaces lock")
            .insert(name, server);
        Ok(())
    }

    /// From now on the host listens on `addr`: every workspace starts and gets its routes.
    pub fn listening(&self, addr: SocketAddr) -> Result<()> {
        let _ = self.listen.set(addr);
        for (name, server) in self.list() {
            self.serve_one(addr, &name, &server)?;
        }
        Ok(())
    }

    fn serve_one(
        &self,
        addr: SocketAddr,
        name: &WorkspaceName,
        server: &Arc<Server>,
    ) -> Result<()> {
        server.start_background(addr, &server.home.route_base())?;
        self.routes
            .write()
            .expect("routes lock")
            .insert(name.clone(), crate::api::router(server.clone()));
        Ok(())
    }

    /// Creates workspace `name` (see [`create_workspace`]), starts it and adds it. A workspace
    /// that doesn't start is removed again.
    pub async fn create(
        &self,
        name: &WorkspaceName,
        template: Option<&TemplateName>,
    ) -> Result<Arc<Server>> {
        if self.workspace(name).is_some() {
            return Err(ServerError::Invalid(format!(
                "workspace {name} exists already; pick another name"
            )));
        }
        let workspace = create_workspace(&self.home, name, template).await?;
        let starter = self.starter.read().expect("starter lock").clone();
        let started = match starter(workspace.clone()).await {
            Ok(server) => self.add(name.clone(), server.clone()).map(|()| server),
            Err(e) => Err(e),
        };
        if started.is_err() {
            let _ = std::fs::remove_dir_all(workspace.root());
        }
        started
    }
}

/// Claims the projects `server`'s config names for its workspace.
fn claim_configured_projects(name: &WorkspaceName, server: &Server) -> Result<()> {
    let config = server.config();
    for key in config.projects.settings.keys() {
        if ProjectPath::resolve(key, &config.projects.dir).is_err() {
            tracing::warn!("workspace {name}: [projects.\"{key}\"] is not a folder; skipped");
            continue;
        }
        server.use_project(key).map_err(|e| {
            ServerError::Invalid(format!(
                "workspace {name}'s config.toml names [projects.\"{key}\"], but {e}"
            ))
        })?;
    }
    Ok(())
}
