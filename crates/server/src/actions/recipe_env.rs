use crate::sandbox::SandboxSpec;
use crate::server::Server;
use crate::session::Session;

/// What a recipe is told through its environment: its site, the session that ran it and that
/// session's project, and the workspace it belongs to (`COMPOSER_WORKSPACE`, ...), so a
/// `composer` command it runs opens its own workspace, never the one the server was started from.
/// Built only from the server that runs the recipe, with the sandbox it runs in: the workspace's
/// sites and that project read-write.
#[derive(Debug, Clone)]
pub struct RecipeEnv {
    vars: Vec<(&'static str, String)>,
    sandbox: SandboxSpec,
}

impl RecipeEnv {
    /// For a recipe of `site` in `server`'s workspace, run by `session` if any. A session's
    /// project another workspace owns is left out.
    pub fn new(server: &Server, site: &str, session: Option<&Session>) -> Self {
        let project = session
            .and_then(|s| s.info().project)
            .and_then(|p| server.see_project(&p).ok())
            .unwrap_or_default();
        Self::with_project(server, site, session, project)
    }

    /// For a recipe an app runs: its own site's, on the project it was opened on.
    /// It also gets `PROJECT_ROOT`, `PROJECT_NAME` and `APP_DATA` (its data folder, made now and
    /// writable in the sandbox).
    pub fn for_app(server: &Server, grant: &crate::apps::AppGrant) -> Self {
        let project = server
            .see_project(&grant.folder.display().to_string())
            .unwrap_or_default();
        let data = grant.data(&server.home);
        if let Err(e) = std::fs::create_dir_all(&data) {
            tracing::warn!("{}: {e}", data.display());
        }
        let mut env = Self::with_project(server, &grant.site, None, project.clone());
        env.vars.extend([
            ("PROJECT_ROOT", project),
            ("PROJECT_NAME", grant.project.to_string()),
            ("APP_DATA", data.display().to_string()),
        ]);
        // Composer's own choice of a folder in the workspace's state: mounted, not a bind.
        env.sandbox = env.sandbox.with_mount(crate::sandbox::Mount::Rw(data));
        env
    }

    fn with_project(
        server: &Server,
        site: &str,
        session: Option<&Session>,
        project: String,
    ) -> Self {
        let mut vars = vec![
            ("COMPOSER_SITE", site.to_string()),
            (
                "COMPOSER_SESSION",
                session.map(|s| s.id().to_string()).unwrap_or_default(),
            ),
            ("COMPOSER_PROJECT", project.clone()),
            (
                "COMPOSER_SDK",
                server.home.templates().join("sdk").display().to_string(),
            ),
        ];
        vars.extend(server.home.env());
        let mut sandbox = server.sandbox();
        if !project.is_empty() {
            sandbox = sandbox.with_rw(project);
        }
        Self { vars, sandbox }
    }

    /// The sandbox the recipe runs in.
    pub fn sandbox(&self) -> &SandboxSpec {
        &self.sandbox
    }

    /// The variables to set, by name.
    pub fn vars(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.vars.iter().map(|(k, v)| (*k, v.as_str()))
    }
}
