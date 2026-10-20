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

/// The model `harness` runs in a project with `settings`: its configured `default` if the
/// project's `models` allow it, else the first model they allow for it. `None`: the harness's
/// own default.
pub fn pick_model(
    settings: &ProjectSettings,
    harness: &str,
    default: Option<&str>,
) -> Result<Option<String>, String> {
    let default = default.filter(|m| !m.is_empty()).map(str::to_string);
    if settings.models.is_empty() || settings.models.iter().any(|m| m == harness) {
        return Ok(default);
    }
    let allowed: Vec<&str> = settings
        .models
        .iter()
        .filter_map(|m| m.strip_prefix(&format!("{harness}:")))
        .collect();
    match (&default, allowed.first()) {
        (Some(d), _) if allowed.contains(&d.as_str()) => Ok(default),
        (_, Some(first)) => Ok(Some(first.to_string())),
        (_, None) => Err(format!(
            "this project allows only these models: {}; none of them runs on `{harness}`",
            settings.models.join(", ")
        )),
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

    #[test]
    fn models() {
        let settings = |models: &[&str]| ProjectSettings {
            models: models.iter().map(|m| m.to_string()).collect(),
            ..Default::default()
        };
        let pick = |models: &[&str], default: Option<&str>| {
            pick_model(&settings(models), "claude-code", default)
        };
        assert_eq!(pick(&[], Some("opus")), Ok(Some("opus".into())));
        assert_eq!(pick(&["claude-code"], None), Ok(None));
        assert_eq!(
            pick(&["claude-code:sonnet"], Some("opus")),
            Ok(Some("sonnet".into()))
        );
        assert_eq!(
            pick(&["claude-code:sonnet", "claude-code:opus"], Some("opus")),
            Ok(Some("opus".into()))
        );
        assert!(
            pick(&["composer"], None)
                .unwrap_err()
                .contains("none of them runs on `claude-code`")
        );
    }
}
