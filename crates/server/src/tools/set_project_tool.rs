use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::SetProjectArgs;
use crate::tools::{Tool, ToolContext};

pub struct SetProjectTool;

#[async_trait]
impl Tool for SetProjectTool {
    fn name(&self) -> &'static str {
        "set_project"
    }

    fn description(&self) -> &'static str {
        "Set the session's active project: a code directory on disk the user is working on. \
         Its project page (site://projects/<name>) is then loaded into your context. \
         Accepts a page, a plain name (a folder in the user's projects directory), or a path. \
         An empty path clears it."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": {
                "type": "string",
                "description": "a project name (folder in the projects directory), an absolute path, or ~/..."
            } }
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: SetProjectArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let project = match args
            .path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            None => None,
            Some(path) => {
                let project = crate::project::ProjectRef::resolve(&context.server, path).map_err(
                    |error| {
                        let dir = expand_home(&context.server.config().projects.dir);
                        let mut names: Vec<String> = std::fs::read_dir(&dir)
                            .into_iter()
                            .flatten()
                            .flatten()
                            .filter_map(|entry| {
                                entry
                                    .file_name()
                                    .to_str()
                                    .filter(|_| entry.path().is_dir())
                                    .map(str::to_string)
                            })
                            .collect();
                        names.sort();
                        names.truncate(12);
                        if names.is_empty() {
                            error
                        } else {
                            format!(
                                "{error}. Projects in {}: {}",
                                dir.display(),
                                names.join(", ")
                            )
                        }
                    },
                )?;
                let root = project.root.display().to_string();
                Some(context.server.use_project(&root)?)
            }
        };
        let message = match &project {
            Some(path) => format!("active project: {path}"),
            None => "active project cleared".to_string(),
        };
        context
            .session
            .update_info(&context.server.hub, |info| info.project = project);
        Ok(message)
    }
}

/// A project directory: absolute, `~/...`, or a name (or relative path) inside
/// `projects_dir`. Requires an existing directory, returns the canonical path.
pub fn resolve_project(path: &str, projects_dir: &str) -> Result<String, String> {
    crate::project::ProjectPath::resolve(path, projects_dir).map(|p| p.to_string())
}

/// `~` and `~/x` → under `$HOME`.
pub fn expand_home(path: &str) -> PathBuf {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
    };
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

/// The reverse, for showing paths: under `$HOME` → `~/x`.
pub fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_folders_in_the_projects_directory() {
        let projects = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(projects.path().join("work/api")).unwrap();
        let dir = projects.path().to_str().unwrap();
        let canonical = |p: &str| {
            projects
                .path()
                .join(p)
                .canonicalize()
                .unwrap()
                .display()
                .to_string()
        };
        assert_eq!(resolve_project("work", dir), Ok(canonical("work")));
        assert_eq!(resolve_project("work/api", dir), Ok(canonical("work/api")));
        let absolute = canonical("work/api");
        assert_eq!(
            resolve_project(&absolute, "/elsewhere"),
            Ok(absolute.clone())
        );
        assert!(resolve_project("nope", dir).unwrap_err().contains("nope"));
    }
}
