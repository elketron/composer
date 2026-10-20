//! Which harnesses a project allows: `[projects."<folder>"]` in config.toml.

use std::path::Path;

use crate::config::Config;
use crate::config_sections::ProjectSettings;
use crate::tools::{resolve_project, tilde};

/// The settings for `project` (a resolved folder): those of the closest configured folder
/// that contains it. Default (nothing allowed) when none does.
pub fn project_settings(config: &Config, project: &str) -> ProjectSettings {
    let project = Path::new(project);
    config
        .projects
        .settings
        .iter()
        .filter_map(|(key, settings)| {
            let folder = resolve_project(key, &config.projects.dir).ok()?;
            project
                .starts_with(&folder)
                .then_some((folder.len(), settings))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, settings)| settings.clone())
        .unwrap_or_default()
}

/// `Ok` when tickets in `project` may run on the external harness `harness`.
pub fn check_harness_allowed(
    config: &Config,
    project: Option<&str>,
    harness: &str,
) -> Result<ProjectSettings, String> {
    let Some(project) = project else {
        return Err(format!(
            "`{harness}` works in a project folder; give the ticket a `project:` (or set one for this session)"
        ));
    };
    let settings = project_settings(config, project);
    if settings.external_harnesses.iter().any(|h| h == harness) {
        Ok(settings)
    } else {
        Err(format!(
            "project {} does not allow the `{harness}` harness; to allow it, add to config.toml:\n\
             [projects.\"{}\"]\nexternal_harnesses = [\"{harness}\"]",
            tilde(Path::new(project)),
            tilde(Path::new(project)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_configured_folder_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let (outer, inner) = (tmp.path().join("p"), tmp.path().join("p/sub"));
        std::fs::create_dir_all(&inner).unwrap();
        let config = Config::parse(&format!(
            "[projects.\"{}\"]\nexternal_harnesses = [\"claude-code\"]\n\n[projects.\"{}\"]\nexternal_harnesses = []\n",
            outer.display(),
            inner.display()
        ))
        .unwrap();
        let canon = |p: &Path| p.canonicalize().unwrap().display().to_string();
        assert!(check_harness_allowed(&config, Some(&canon(&outer)), "claude-code").is_ok());
        let deeper = inner.join("x");
        std::fs::create_dir_all(&deeper).unwrap();
        let refused = check_harness_allowed(&config, Some(&canon(&deeper)), "claude-code");
        assert!(refused.unwrap_err().contains("does not allow"));
        assert!(check_harness_allowed(&config, None, "claude-code").is_err());
        assert!(
            check_harness_allowed(&Config::defaults(), Some(&canon(&outer)), "claude-code")
                .is_err()
        );
    }
}
