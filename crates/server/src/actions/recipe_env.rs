use composer_core::SiteUrl;

use crate::project::ProjectMode;
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
        // `PROJECT_ROOT` is an explicit grant: only a site that may write gets it. A `read`
        // site still gets `COMPOSER_PROJECT` (the path), but not `PROJECT_ROOT`.
        let mode = SiteUrl::new(site, "")
            .ok()
            .and_then(|url| server.web.read_page(&url).ok())
            .and_then(|page| page.frontmatter.get_str("project_mode"))
            .map(|raw| ProjectMode::parse(&raw))
            .unwrap_or_default();
        let project_root = (!project.is_empty() && !mode.readonly()).then(|| project.clone());
        Self::with_project(server, site, session, project, project_root)
    }

    /// For a recipe an app runs: its own site's, on the project it was opened on.
    /// It also gets `PROJECT_ROOT`, `PROJECT_NAME` and `APP_DATA` (its data folder, made now and
    /// writable in the sandbox).
    pub fn for_app(server: &Server, grant: &crate::apps::AppGrant) -> Self {
        let project = grant
            .project
            .as_ref()
            .and_then(|p| server.see_project(&p.folder.display().to_string()).ok())
            .unwrap_or_default();
        let data = grant.data(&server.home);
        if let Err(e) = std::fs::create_dir_all(&data) {
            tracing::warn!("{}: {e}", data.display());
        }
        let mut env = Self::with_project(
            server,
            &grant.site,
            None,
            project.clone(),
            Some(project.clone()),
        );
        env.vars.extend([
            (
                "PROJECT_NAME",
                grant
                    .project
                    .as_ref()
                    .map(|p| p.name.to_string())
                    .unwrap_or_default(),
            ),
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
        project_root: Option<String>,
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
        if let Some(root) = project_root {
            vars.push(("PROJECT_ROOT", root));
        }
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
