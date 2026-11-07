use std::fmt;
use std::path::PathBuf;

use crate::apps::{ProjectName, find_project};
use crate::server::Server;

/// The Home's place in a `data://` URL: project names can't start with `@`.
const HOME: &str = "@home";

/// An app's work data, as an agent names it (ticket 0171): `data://<project>/<app>/<path>`
/// (`data://@home/<app>/...` for an app opened on Home). `data://<project>/` names the apps with
/// data there; without a path, the app's whole folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataUrl {
    /// `None`: Home.
    pub project: Option<ProjectName>,
    pub app: Option<String>,
    pub path: String,
}

impl DataUrl {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let wrong = || {
            format!(
                "`{raw}` isn't a data URL: `data://<project>/<app>/<path>` (an app's data for a \
                 project), `data://@home/<app>/<path>` (on Home), or `data://<project>/` (which \
                 apps have data there)"
            )
        };
        let rest = raw.trim().strip_prefix("data://").ok_or_else(wrong)?;
        let mut parts = rest.splitn(3, '/');
        let place = parts.next().filter(|p| !p.is_empty()).ok_or_else(wrong)?;
        let project = match place {
            HOME => None,
            name => Some(ProjectName::new(name)?),
        };
        let app = parts.next().filter(|a| !a.is_empty()).map(str::to_string);
        if app
            .as_deref()
            .is_some_and(|a| a.starts_with('.') || a.contains('\\'))
        {
            return Err(wrong());
        }
        let path = parts
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string();
        Ok(Self { project, app, path })
    }

    /// The folder this names (the project's data, or one app's), in `server`'s workspace: the
    /// project must be one of its own.
    pub fn folder(&self, server: &Server) -> Result<PathBuf, String> {
        let base = match &self.project {
            None => server.home.state().join("home"),
            Some(name) => {
                find_project(server, name.as_str())?;
                server.home.state().join("projects").join(name.as_str())
            }
        };
        Ok(match &self.app {
            Some(app) => base.join(app),
            None => base,
        })
    }
}

impl fmt::Display for DataUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let place = self.project.as_ref().map_or(HOME, |p| p.as_str());
        write!(f, "data://{place}/")?;
        if let Some(app) = &self.app {
            write!(f, "{app}/{}", self.path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_app_path_or_home() {
        let url = DataUrl::parse("data://game/cad/views/arm.json").unwrap();
        assert_eq!(url.project.as_ref().unwrap().as_str(), "game");
        assert_eq!(
            (url.app.as_deref(), url.path.as_str()),
            (Some("cad"), "views/arm.json")
        );
        assert_eq!(url.to_string(), "data://game/cad/views/arm.json");
        let home = DataUrl::parse("data://@home/calendar/").unwrap();
        assert_eq!(
            (home.project, home.app.as_deref(), home.path.as_str()),
            (None, Some("calendar"), "")
        );
        let apps = DataUrl::parse("data://game/").unwrap();
        assert_eq!(apps.app, None);
        for bad in ["site://x", "data://", "data://.x/a", "data://game/.git/x"] {
            assert!(DataUrl::parse(bad).is_err(), "{bad}");
        }
    }
}
