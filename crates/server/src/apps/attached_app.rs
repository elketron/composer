use crate::apps::{AppManifest, AttachReason, ProjectFacts};

/// An app of the workspace (a site with a manifest), in a project's list or on Home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedApp {
    pub site: String,
    pub manifest: AppManifest,
    pub why: AttachReason,
    /// A project-local app's folder.
    pub local: Option<std::path::PathBuf>,
}

impl AttachedApp {
    /// A project's apps: `pins` first (in their order, whatever their scope), then the project or
    /// both-scope apps whose `when` matches `facts`, by site; none of `hidden`. `apps` are the
    /// workspace's, by site.
    pub fn for_project(
        apps: &[(String, AppManifest)],
        facts: &ProjectFacts,
        pins: &[String],
        hidden: &[String],
    ) -> Vec<Self> {
        let shown = |site: &str| !hidden.iter().any(|h| h == site);
        let pinned = pins.iter().filter(|p| shown(p)).filter_map(|pin| {
            let (site, manifest) = apps.iter().find(|(site, _)| site == pin)?;
            Some(Self {
                site: site.clone(),
                manifest: manifest.clone(),
                why: AttachReason::Pinned,
                local: None,
            })
        });
        let mut list: Vec<Self> = pinned.collect();
        let matched = apps
            .iter()
            .filter(|(site, m)| m.scope.in_projects() && shown(site) && !pins.contains(site))
            .filter_map(|(site, manifest)| {
                let hit = manifest.when.iter().find(|c| c.matches(facts))?;
                Some(Self {
                    site: site.clone(),
                    manifest: manifest.clone(),
                    why: AttachReason::Matched(hit.clone()),
                    local: None,
                })
            });
        list.extend(matched);
        list
    }

    /// The workspace's Home: its global and both-scope apps, by site.
    pub fn for_home(apps: &[(String, AppManifest)]) -> Vec<Self> {
        apps.iter()
            .filter(|(_, m)| m.scope.on_home())
            .map(|(site, manifest)| Self {
                site: site.clone(),
                manifest: manifest.clone(),
                why: AttachReason::Home,
                local: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::apps::{AppCondition, AppScope};

    fn app(scope: AppScope, when: &[&str]) -> AppManifest {
        AppManifest {
            scope,
            when: when
                .iter()
                .map(|c| AppCondition::parse(c).unwrap())
                .collect(),
            ..AppManifest::plain()
        }
    }

    fn apps() -> Vec<(String, AppManifest)> {
        vec![
            ("board".into(), app(AppScope::Both, &["git"])),
            (
                "cad".into(),
                app(AppScope::Project, &["pipeline:cad", "files:*.FCStd"]),
            ),
            ("calendar".into(), app(AppScope::Global, &[])),
            ("video".into(), app(AppScope::Project, &["pipeline:video"])),
        ]
    }

    fn sites(list: &[AttachedApp]) -> Vec<(String, String)> {
        list.iter()
            .map(|a| (a.site.clone(), a.why.to_string()))
            .collect()
    }

    #[test]
    fn matches_pins_and_hides() {
        let facts = ProjectFacts::new(true, [], [], vec![PathBuf::from("arm.FCStd")]);
        let none: [String; 0] = [];
        assert_eq!(
            sites(&AttachedApp::for_project(&apps(), &facts, &none, &none)),
            [
                ("board".into(), "matched git".into()),
                ("cad".into(), "matched files:*.FCStd".into())
            ]
        );
        // A pin comes first, whatever its scope; a hidden app is gone even when it matches.
        let pins = ["calendar".to_string(), "nope".to_string()];
        assert_eq!(
            sites(&AttachedApp::for_project(
                &apps(),
                &facts,
                &pins,
                &["board".into()]
            )),
            [
                ("calendar".into(), "pinned".into()),
                ("cad".into(), "matched files:*.FCStd".into())
            ]
        );
        let plain = ProjectFacts::default();
        assert!(AttachedApp::for_project(&apps(), &plain, &none, &none).is_empty());
    }

    #[test]
    fn home_has_global_and_both() {
        assert_eq!(
            sites(&AttachedApp::for_home(&apps())),
            [
                ("board".into(), "home".into()),
                ("calendar".into(), "home".into())
            ]
        );
    }
}
