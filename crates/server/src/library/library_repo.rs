use std::path::{Component, Path, PathBuf};

use composer_core::SiteUrl;

use crate::home::Home;
use crate::library::LibraryCommit;
use crate::project::git::{git, git_output};

/// The library folder (`<home>/sites/`) as a local git repo: every change to it (a stock update,
/// later a publish) is a commit, so an installed copy can be merged against the version it came
/// from.
#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
}

/// Commits are the library's own, whatever the user's git config says.
const IDENTITY: [&str; 6] = [
    "-c",
    "user.name=composer",
    "-c",
    "user.email=composer@localhost",
    "-c",
    "commit.gpgsign=false",
];

/// What a library never tracks, should it show up.
const IGNORE: &str = "node_modules/\ndist/\n";

impl Library {
    /// The library of `home`.
    pub fn of(home: &Home) -> Self {
        Self {
            root: home.library(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Commits everything in the library as it is now, making it a repo first if it isn't one.
    /// Returns the new commit, or `None` when nothing changed.
    pub fn record(&self, message: &str) -> Result<Option<LibraryCommit>, String> {
        std::fs::create_dir_all(&self.root).map_err(|e| format!("{}: {e}", self.root.display()))?;
        if !self.root.join(".git").exists() {
            git(&self.root, &["init", "-q", "-b", "main"])?;
            std::fs::write(self.root.join(".gitignore"), IGNORE)
                .map_err(|e| format!("writing the library's .gitignore: {e}"))?;
        }
        git(&self.root, &["add", "-A"])?;
        if git(&self.root, &["status", "--porcelain"])?.is_empty() {
            return Ok(None);
        }
        let mut args = IDENTITY.to_vec();
        args.extend(["commit", "-q", "-m", message]);
        git(&self.root, &args)?;
        self.head()
    }

    /// The latest commit, `None` before the first.
    pub fn head(&self) -> Result<Option<LibraryCommit>, String> {
        if !self.root.join(".git").exists() {
            return Ok(None);
        }
        match git(&self.root, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
            Ok(sha) => LibraryCommit::parse(&sha).map(Some),
            Err(_) => Ok(None),
        }
    }

    /// The library's sites at HEAD, each with the last commit that changed it.
    pub fn sites(&self) -> Result<Vec<(String, LibraryCommit)>, String> {
        let Some(head) = self.head()? else {
            return Ok(Vec::new());
        };
        let names = git(&self.root, &["ls-tree", "--name-only", "-d", head.as_str()])?;
        names
            .lines()
            .filter(|name| !name.starts_with('.'))
            .map(|name| {
                let last = git(
                    &self.root,
                    &["log", "-1", "--format=%H", head.as_str(), "--", name],
                )?;
                Ok((name.to_string(), LibraryCommit::parse(&last)?))
            })
            .collect()
    }

    /// The files of `site` at `commit`, relative to the site.
    pub fn files_at(&self, commit: &LibraryCommit, site: &str) -> Result<Vec<PathBuf>, String> {
        let site = site_name(site)?;
        let listed = git(
            &self.root,
            &[
                "ls-tree",
                "-r",
                "--name-only",
                commit.as_str(),
                "--",
                &format!("{site}/"),
            ],
        )?;
        Ok(listed
            .lines()
            .filter_map(|line| line.strip_prefix(&format!("{site}/")))
            .map(PathBuf::from)
            .collect())
    }

    /// `path` of `site` as it was at `commit`; `None` when it didn't exist then.
    pub fn read_at(
        &self,
        commit: &LibraryCommit,
        site: &str,
        path: &Path,
    ) -> Result<Option<Vec<u8>>, String> {
        let site = site_name(site)?;
        let path = relative(path)?;
        let spec = format!("{commit}:{site}/{}", path.display());
        let output = git_output(&self.root, &["show", &spec], &[])?;
        Ok(output.status.success().then_some(output.stdout))
    }
}

/// A library site name, by the web's site-name rules.
fn site_name(site: &str) -> Result<&str, String> {
    SiteUrl::new(site, "").map_err(|e| e.to_string())?;
    Ok(site)
}

/// A plain relative path (no `..`, not absolute).
fn relative(path: &Path) -> Result<&Path, String> {
    let plain = path.components().all(|c| matches!(c, Component::Normal(_)));
    if plain && !path.as_os_str().is_empty() {
        Ok(path)
    } else {
        Err(format!("`{}` is not a path inside a site", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_reads_back_and_lists_sites() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::new(dir.path());
        let library = Library::of(&home);
        assert_eq!(library.head().unwrap(), None);
        std::fs::create_dir_all(library.root().join("cad/stages")).unwrap();
        std::fs::write(library.root().join("cad/index.md"), "v1").unwrap();
        std::fs::write(library.root().join("cad/stages/01_a.md"), "a").unwrap();
        let first = library.record("first").unwrap().unwrap();
        assert_eq!(library.record("nothing").unwrap(), None, "no empty commits");

        std::fs::write(library.root().join("cad/index.md"), "v2").unwrap();
        std::fs::create_dir_all(library.root().join("home")).unwrap();
        std::fs::write(library.root().join("home/index.md"), "h").unwrap();
        let second = library.record("second").unwrap().unwrap();
        assert_ne!(first, second);

        let read = |commit: &LibraryCommit, path: &str| {
            library.read_at(commit, "cad", Path::new(path)).unwrap()
        };
        assert_eq!(read(&first, "index.md").as_deref(), Some(&b"v1"[..]));
        assert_eq!(read(&second, "index.md").as_deref(), Some(&b"v2"[..]));
        assert_eq!(read(&first, "gone.md"), None);
        assert_eq!(
            library.files_at(&first, "cad").unwrap(),
            [PathBuf::from("index.md"), PathBuf::from("stages/01_a.md")]
        );
        let sites = library.sites().unwrap();
        assert_eq!(
            sites,
            [
                ("cad".to_string(), second.clone()),
                ("home".to_string(), second)
            ]
        );
        // Nothing outside a site, nothing that isn't a plain path.
        assert!(library.read_at(&first, "..", Path::new("x")).is_err());
        assert!(library.read_at(&first, "cad", Path::new("../x")).is_err());
        assert!(
            library
                .read_at(&first, "cad", Path::new("/etc/passwd"))
                .is_err()
        );
    }
}
