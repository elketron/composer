use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::apps::FileGlob;

/// Folders a file walk never enters: version control, dependencies and build output.
pub(crate) const SKIPPED: [&str; 8] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    ".composer",
];
/// A walk stops after this many files, or this deep: matching is a hint, not an inventory.
const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 6;

/// What an app's `when` is matched against: whether the project is a git repository, the
/// pipelines with a work linking it, its tags (`[projects."<path>"] tags`), and its files (a
/// bounded walk).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectFacts {
    git: bool,
    pipelines: BTreeSet<String>,
    tags: BTreeSet<String>,
    /// Relative to the project.
    files: Vec<PathBuf>,
}

impl ProjectFacts {
    pub fn new(
        git: bool,
        pipelines: impl IntoIterator<Item = String>,
        tags: impl IntoIterator<Item = String>,
        files: Vec<PathBuf>,
    ) -> Self {
        Self {
            git,
            pipelines: pipelines.into_iter().collect(),
            tags: tags.into_iter().collect(),
            files,
        }
    }

    /// The facts of `folder`; `pipelines` and `tags` come from the workspace.
    pub fn gather(
        folder: &Path,
        pipelines: impl IntoIterator<Item = String>,
        tags: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new(folder.join(".git").exists(), pipelines, tags, files(folder))
    }

    pub fn is_git(&self) -> bool {
        self.git
    }

    pub fn has_pipeline(&self, site: &str) -> bool {
        self.pipelines.contains(site)
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }

    pub fn has_file(&self, glob: &FileGlob) -> bool {
        self.files.iter().any(|f| glob.matches(f))
    }
}

fn files(folder: &Path) -> Vec<PathBuf> {
    let walk = walkdir::WalkDir::new(folder)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !SKIPPED.iter().any(|s| e.file_name() == *s));
    walk.flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.path().strip_prefix(folder).ok().map(Path::to_path_buf))
        .take(MAX_FILES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_skips_dependencies_and_build_output() {
        let dir = tempfile::tempdir().unwrap();
        for file in [
            "a.FCStd",
            "parts/b.step",
            "node_modules/x/c.FCStd",
            "target/d.FCStd",
        ] {
            let path = dir.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let facts = ProjectFacts::gather(dir.path(), [], []);
        assert!(!facts.is_git());
        let mut found = facts.files.clone();
        found.sort();
        assert_eq!(
            found,
            [PathBuf::from("a.FCStd"), PathBuf::from("parts/b.step")]
        );
    }
}
