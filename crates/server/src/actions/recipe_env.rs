use crate::server::Server;
use crate::session::Session;

/// What a recipe is told through its environment: its site, the session that ran it and that
/// session's project, and the home it belongs to (`COMPOSER_HOME`), so a `composer` command it
/// runs opens the same home. Built only from the server that runs the recipe.
#[derive(Debug, Clone)]
pub struct RecipeEnv {
    vars: Vec<(&'static str, String)>,
}

impl RecipeEnv {
    /// For a recipe of `site`, run by `session` if any.
    pub fn new(server: &Server, site: &str, session: Option<&Session>) -> Self {
        let project = session
            .and_then(|s| s.info().project)
            .and_then(|p| server.project_folder(&p).ok())
            .unwrap_or_default();
        let project_root = (!project.is_empty()).then(|| project.clone());
        Self::with_project(server, site, session, project, project_root)
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
        ];
        if let Some(root) = project_root {
            vars.push(("PROJECT_ROOT", root));
        }
        vars.extend(server.home.env());
        Self { vars }
    }

    /// The variables to set, by name.
    pub fn vars(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.vars.iter().map(|(k, v)| (*k, v.as_str()))
    }
}
