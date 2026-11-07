use std::path::{Component, Path, PathBuf};

use crate::library::site_definition::NEVER;

/// One path a site's definition covers: a core path (`ui`), a `library:` entry naming a file or
/// folder of the site (`tools/`), or one whose last part is a simple glob (`*.md`, `agents/*.md`),
/// matching files in that one folder (not recursive). Always inside the site: no `..`, no `/` at the
/// start, no hidden names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryEntry {
    /// The folder of a glob, or the path itself.
    path: PathBuf,
    /// The glob on the last part (`*.md`), matched against names in `path`.
    glob: Option<String>,
}

impl LibraryEntry {
    /// The entry as written under `library:`; refused when it leaves the site, names a hidden file,
    /// or has a glob anywhere but the last part.
    pub fn parse(entry: &str) -> Result<Self, String> {
        let refused = || {
            format!(
                "`library: {entry}` is not a path inside the site: list folders or files of the \
                 site itself (`tools/`, `comfy-workflows/`, `*.md`), without `..`, `/` at the \
                 start, hidden names, or `*` before the last part"
            )
        };
        let full = PathBuf::from(entry.trim().trim_end_matches('/'));
        let parts: Option<Vec<String>> = full
            .components()
            .map(|c| match c {
                Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                _ => None,
            })
            .collect();
        let parts = parts.filter(|p| !p.is_empty()).ok_or_else(refused)?;
        if parts.iter().any(|p| p.starts_with('.')) {
            return Err(refused());
        }
        let (last, folder) = parts.split_last().ok_or_else(refused)?;
        if folder.iter().any(|p| p.contains('*')) {
            return Err(refused());
        }
        if !last.contains('*') {
            return Ok(Self::path(full));
        }
        Ok(Self {
            path: folder.iter().collect(),
            glob: Some(last.clone()),
        })
    }

    /// A plain path of the site (`ui`).
    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            glob: None,
        }
    }

    /// Whether the definition this entry is part of covers `rel` (a file path relative to the
    /// site): it is the path, under it, or a file the glob matches; never with a hidden or
    /// `node_modules`/`dist` part.
    pub fn covers(&self, rel: &Path) -> bool {
        let excluded = rel.components().any(|c| {
            let part = c.as_os_str().to_string_lossy();
            part.starts_with('.') || NEVER.contains(&part.as_ref())
        });
        if excluded {
            return false;
        }
        match &self.glob {
            None => rel.starts_with(&self.path),
            Some(glob) => {
                rel.parent() == Some(self.path.as_path())
                    && rel
                        .file_name()
                        .is_some_and(|name| matches(glob, &name.to_string_lossy()))
            }
        }
    }

    /// Whether the top-level entry `name` of the site is (or holds) something this entry covers.
    pub fn reaches(&self, name: &Path, is_dir: bool) -> bool {
        match &self.glob {
            Some(glob) if self.path.as_os_str().is_empty() => {
                !is_dir && matches(glob, &name.to_string_lossy())
            }
            _ => self.path.starts_with(name),
        }
    }

    /// The paths in `site` this entry names that exist: the path itself, or each file the glob
    /// matches.
    pub fn existing(&self, site: &Path) -> Vec<PathBuf> {
        let Some(glob) = &self.glob else {
            return match std::fs::symlink_metadata(site.join(&self.path)) {
                Ok(_) => vec![self.path.clone()],
                Err(_) => Vec::new(),
            };
        };
        let mut found: Vec<PathBuf> = std::fs::read_dir(site.join(&self.path))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| matches(glob, name))
            .map(|name| self.path.join(name))
            .collect();
        found.sort();
        found
    }

    /// A glob may match nothing; a plain path is expected to exist.
    pub fn is_glob(&self) -> bool {
        self.glob.is_some()
    }

    /// The entry as listed.
    pub fn display(&self) -> PathBuf {
        match &self.glob {
            Some(glob) => self.path.join(glob),
            None => self.path.clone(),
        }
    }
}

/// `name` against a glob of literal text and `*` (any run of characters).
fn matches(glob: &str, name: &str) -> bool {
    let mut parts = glob.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let pieces: Vec<&str> = parts.collect();
    let Some((last, middle)) = pieces.split_last() else {
        return rest.is_empty();
    };
    for piece in middle {
        let Some(at) = rest.find(piece) else {
            return false;
        };
        rest = &rest[at + piece.len()..];
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_one_folders_files() {
        let md = LibraryEntry::parse("*.md").unwrap();
        assert!(md.covers(Path::new("default.md")));
        assert!(!md.covers(Path::new("sub/default.md")));
        assert!(!md.covers(Path::new("default.txt")));
        assert!(!md.covers(Path::new(".hidden.md")));
        let nested = LibraryEntry::parse("agents/*-v*.md").unwrap();
        assert!(nested.covers(Path::new("agents/a-v2.md")));
        assert!(!nested.covers(Path::new("agents/a.md")));
        assert!(!nested.covers(Path::new("a-v2.md")));
        assert!(matches("*", "x") && matches("a*", "a") && !matches("a*b", "ab_"));
        assert!(!matches("ab*ba", "aba"));
    }

    #[test]
    fn paths_cover_what_is_under_them_but_never_content_builds() {
        let tools = LibraryEntry::parse("tools/").unwrap();
        assert!(tools.covers(Path::new("tools/run.py")));
        assert!(tools.covers(Path::new("tools")));
        assert!(!tools.covers(Path::new("toolsx/run.py")));
        assert!(!tools.covers(Path::new("tools/node_modules/x.js")));
        assert!(!tools.covers(Path::new("tools/.env")));
    }

    #[test]
    fn entries_stay_inside_the_site() {
        for bad in [
            "../other",
            "/etc",
            "tools/../../x",
            ".git",
            "a/.hidden",
            "",
            "*/x.md",
            "../*.md",
            ".*",
        ] {
            assert!(LibraryEntry::parse(bad).is_err(), "{bad}");
        }
    }
}
