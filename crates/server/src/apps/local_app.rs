use std::path::{Path, PathBuf};

use composer_core::{Page, SiteUrl};
use sha2::{Digest, Sha256};

use crate::apps::AppManifest;

/// A project-local app: `<project>/.composer/apps/<name>/` with an `index.md` (an `app:` block, or
/// a `ui/`), a `justfile`, a `ui/`. It comes with the project (a clone brings it), so it is off
/// until the user allows it ([`crate::apps::LocalAllowances`]); the allowance is for this exact
/// `index.md` and `justfile` ([`Self::hash`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalApp {
    pub name: String,
    pub dir: PathBuf,
    pub manifest: AppManifest,
    /// Of its `index.md` and `justfile`: what an allowance is for.
    pub hash: String,
}

impl LocalApp {
    /// The local apps of the project in `folder`, by name; and those whose `app:` block doesn't
    /// parse (`<project>/.composer/apps/<name>/index.md: ...`).
    pub fn discover(folder: &Path) -> (Vec<Self>, Vec<String>) {
        let root = folder.join(".composer/apps");
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        entries.sort();
        let (mut apps, mut errors) = (Vec::new(), Vec::new());
        for dir in entries {
            match Self::load(&dir) {
                Ok(Some(app)) => apps.push(app),
                Ok(None) => {}
                Err(e) => errors.push(format!("{}/index.md `app:`: {e}", dir.display())),
            }
        }
        (apps, errors)
    }

    fn load(dir: &Path) -> Result<Option<Self>, String> {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let url = SiteUrl::new(&name, "index.md")
            .map_err(|_| format!("`{name}` isn't an app name (letters, digits, - and _)"))?;
        let index = std::fs::read_to_string(dir.join("index.md")).unwrap_or_default();
        let page = Page::parse(url, index.clone());
        let Some(manifest) =
            AppManifest::of_site(page.frontmatter.get("app"), dir.join("ui").is_dir())?
        else {
            return Ok(None);
        };
        let justfile = std::fs::read(dir.join("justfile")).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(index.as_bytes());
        hasher.update([0]);
        hasher.update(&justfile);
        Ok(Some(Self {
            name,
            dir: dir.to_path_buf(),
            manifest,
            hash: hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_in_the_project_and_hashed_by_what_it_runs() {
        let project = tempfile::tempdir().unwrap();
        let viewer = project.path().join(".composer/apps/viewer");
        std::fs::create_dir_all(viewer.join("ui")).unwrap();
        std::fs::write(viewer.join("index.md"), "---\ntitle: Viewer\n---\n").unwrap();
        std::fs::write(viewer.join("justfile"), "a:\n    echo a\n").unwrap();
        let broken = project.path().join(".composer/apps/broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(
            broken.join("index.md"),
            "---\napp:\n  scope: nowhere\n---\n",
        )
        .unwrap();
        std::fs::create_dir_all(project.path().join(".composer/apps/empty")).unwrap();

        let (apps, errors) = LocalApp::discover(project.path());
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "viewer");
        assert!(
            errors.len() == 1 && errors[0].contains("broken"),
            "{errors:?}"
        );
        let before = apps[0].hash.clone();
        std::fs::write(viewer.join("justfile"), "a:\n    echo b\n").unwrap();
        assert_ne!(LocalApp::discover(project.path()).0[0].hash, before);
    }
}
