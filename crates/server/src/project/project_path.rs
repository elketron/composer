use std::fmt;
use std::path::{Path, PathBuf};

use crate::tools::{expand_home, tilde};

/// A project folder as the server knows it: canonical (symlinks and `..` resolved), absolute,
/// existing, a directory. Only [`ProjectPath::resolve`] and [`ProjectPath::existing`] make one,
/// so every ownership comparison is between canonical paths: a symlink or `../` spelling can't
/// slip past a check that a raw path would fail.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectPath(PathBuf);

impl ProjectPath {
    /// A project named as a user or agent would: absolute, `~/...`, or a name (or relative
    /// path) inside the projects directory.
    pub fn resolve(path: &str, projects_dir: &str) -> Result<Self, String> {
        let expanded = expand_home(path);
        let expanded = if expanded.is_relative() {
            expand_home(projects_dir).join(expanded)
        } else {
            expanded
        };
        Self::existing(&expanded).map_err(|e| {
            if e.ends_with("is not a directory") {
                format!("{path} is not a directory")
            } else {
                e
            }
        })
    }

    /// An existing folder, canonicalized.
    pub fn existing(path: &Path) -> Result<Self, String> {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", tilde(path)))?;
        if !canonical.is_dir() {
            return Err(format!("{} is not a directory", tilde(path)));
        }
        Ok(Self(canonical))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// One contains the other (or they are the same): two such projects can't belong to
    /// different workspaces.
    pub fn overlaps(&self, other: &ProjectPath) -> bool {
        self.0.starts_with(&other.0) || other.0.starts_with(&self.0)
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_of_one_folder_are_one_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("work/api")).unwrap();
        std::os::unix::fs::symlink(root.join("work"), root.join("link")).unwrap();
        let projects = root.display().to_string();
        let plain = ProjectPath::resolve("work", &projects).unwrap();
        for spelling in ["link", "work/api/..", "./work", &format!("{projects}/work")] {
            assert_eq!(
                ProjectPath::resolve(spelling, &projects).unwrap(),
                plain,
                "{spelling}"
            );
        }
        assert!(
            ProjectPath::resolve("nope", &projects)
                .unwrap_err()
                .contains("nope")
        );
    }

    #[test]
    fn nested_folders_overlap_both_ways() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for d in ["x/sub", "xy"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let p = |d: &str| ProjectPath::existing(&root.join(d)).unwrap();
        assert!(p("x").overlaps(&p("x/sub")));
        assert!(p("x/sub").overlaps(&p("x")));
        assert!(p("x").overlaps(&p("x")));
        assert!(
            !p("x").overlaps(&p("xy")),
            "a shared name prefix is not nesting"
        );
    }
}
