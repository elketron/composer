use crate::server::Server;
use crate::session::Session;

/// What a recipe is told through its environment: its site, the session that ran it and the
/// folder of that session's site (`COMPOSER_FOLDER`), and the home it belongs to (`COMPOSER_HOME`), so a `composer` command it
/// runs opens the same home. Built only from the server that runs the recipe.
#[derive(Debug, Clone)]
pub struct RecipeEnv {
    vars: Vec<(&'static str, String)>,
}

impl RecipeEnv {
    /// For a recipe of `site`, run by `session` if any.
    pub fn new(server: &Server, site: &str, session: Option<&Session>) -> Self {
        let folder = session
            .and_then(|s| server.session_folder(&s.info()))
            .map(|folder| folder.display().to_string())
            .unwrap_or_default();
        let mut vars = vec![
            ("COMPOSER_SITE", site.to_string()),
            (
                "COMPOSER_SESSION",
                session.map(|s| s.id().to_string()).unwrap_or_default(),
            ),
            ("COMPOSER_FOLDER", folder),
        ];
        vars.extend(server.home.env());
        Self { vars }
    }

    /// The variables to set, by name.
    pub fn vars(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.vars.iter().map(|(k, v)| (*k, v.as_str()))
    }
}
