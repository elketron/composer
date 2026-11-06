use crate::server::Server;
use crate::session::Session;

/// What a recipe is told through its environment: its site, the session that ran it and that
/// session's project, and the workspace it belongs to (`COMPOSER_WORKSPACE`, ...), so a
/// `composer` command it runs opens its own workspace, never the one the server was started from.
/// Built only from the server that runs the recipe.
#[derive(Debug, Clone)]
pub struct RecipeEnv {
    vars: Vec<(&'static str, String)>,
}

impl RecipeEnv {
    /// For a recipe of `site` in `server`'s workspace, run by `session` if any. A session's
    /// project another workspace owns is left out.
    pub fn new(server: &Server, site: &str, session: Option<&Session>) -> Self {
        let project = session
            .and_then(|s| s.info().project)
            .and_then(|p| server.see_project(&p).ok())
            .unwrap_or_default();
        let mut vars = vec![
            ("COMPOSER_SITE", site.to_string()),
            (
                "COMPOSER_SESSION",
                session.map(|s| s.id().to_string()).unwrap_or_default(),
            ),
            ("COMPOSER_PROJECT", project),
        ];
        vars.extend(server.home.env());
        Self { vars }
    }

    /// The variables to set, by name.
    pub fn vars(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.vars.iter().map(|(k, v)| (*k, v.as_str()))
    }
}
