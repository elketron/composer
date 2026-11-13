use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use composer_core::{Page, SiteUrl};
use serde_json::Value;

use crate::library::LibraryEntry;

/// The part of a site that is its definition, and so what the library holds, installs and
/// publishes: `index.md`, `justfile`, `ui/` (source only), and every path the index lists under
/// `library:` (`library: [tools/, comfy-workflows/, "*.md"]`).
///
/// Never in it: works, tickets and other pages outside those paths; `node_modules/` and
/// `dist/` anywhere; hidden files; symlinks (not followed, not copied). A `library:` entry that
/// leaves the site is refused, so a definition can't carry anything from outside its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteDefinition {
    /// The paths it covers: the core and the `library:` entries.
    entries: Vec<LibraryEntry>,
    /// Files, relative to the site folder.
    files: BTreeSet<PathBuf>,
    /// Top-level entries of the site that are not part of it (works, tickets, pages, ...).
    left_out: Vec<PathBuf>,
    /// `library:` entries that don't exist.
    missing: Vec<PathBuf>,
}

/// Always part of a definition, when present.
const CORE: [&str; 3] = ["index.md", "justfile", "ui"];
/// Never part of one, at any depth.
pub(crate) const NEVER: [&str; 2] = ["node_modules", "dist"];

impl SiteDefinition {
    /// The definition of the site in `dir`.
    pub fn read(dir: &Path) -> Result<Self, String> {
        let mut entries: Vec<LibraryEntry> = CORE.iter().map(LibraryEntry::path).collect();
        let extras = extras(dir)?;
        let missing = extras
            .iter()
            .filter(|e| !e.is_glob() && e.existing(dir).is_empty())
            .map(LibraryEntry::display)
            .collect();
        entries.extend(extras);
        let mut files = BTreeSet::new();
        for path in entries.iter().flat_map(|e| e.existing(dir)) {
            collect(dir, &dir.join(path), &mut files);
        }
        let left_out = top_level(dir)
            .into_iter()
            .filter(|(name, is_dir)| !entries.iter().any(|e| e.reaches(name, *is_dir)))
            .map(|(name, _)| name)
            .collect();
        Ok(Self {
            entries,
            files,
            left_out,
            missing,
        })
    }

    pub fn files(&self) -> &BTreeSet<PathBuf> {
        &self.files
    }

    pub fn left_out(&self) -> &[PathBuf] {
        &self.left_out
    }

    pub fn missing(&self) -> &[PathBuf] {
        &self.missing
    }

    /// Whether `rel` (a file path relative to the site) is one this definition is about, so a
    /// publish may replace or remove it; anything else in a library site is left as it is.
    pub fn covers(&self, rel: &Path) -> bool {
        self.entries.iter().any(|e| e.covers(rel))
    }
}

/// The index's `library:` entries, each a path (or glob) inside the site.
fn extras(dir: &Path) -> Result<Vec<LibraryEntry>, String> {
    let Ok(raw) = std::fs::read_to_string(dir.join("index.md")) else {
        return Ok(Vec::new());
    };
    let url = SiteUrl::new("site", "index.md").map_err(|e| e.to_string())?;
    let page = Page::parse(url, raw);
    let entries: Vec<String> = match page.frontmatter.get("library") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(one)) => vec![one.clone()],
        _ => return Ok(Vec::new()),
    };
    entries
        .iter()
        .map(|entry| LibraryEntry::parse(entry))
        .collect()
}

/// Adds the files at `path` (a file, or a folder walked without following symlinks).
fn collect(site: &Path, path: &Path, files: &mut BTreeSet<PathBuf>) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    let name = path.file_name().map(|n| n.to_string_lossy().to_string());
    let skipped = name
        .as_deref()
        .is_some_and(|n| n.starts_with('.') || NEVER.contains(&n));
    if skipped || meta.file_type().is_symlink() {
        return;
    }
    if meta.is_file() {
        if let Ok(rel) = path.strip_prefix(site) {
            files.insert(rel.to_path_buf());
        }
        return;
    }
    for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
        collect(site, &entry.path(), files);
    }
}

/// The site's top-level entries (not hidden), each with whether it is a folder.
fn top_level(dir: &Path) -> Vec<(PathBuf, bool)> {
    let mut names: Vec<(PathBuf, bool)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (PathBuf::from(e.file_name()), e.path().is_dir()))
        .filter(|(n, _)| !n.to_string_lossy().starts_with('.'))
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn the_core_and_listed_extras_without_content() {
        let dir = tempfile::tempdir().unwrap();
        let site = dir.path();
        write(
            site,
            "index.md",
            "---\ntitle: Cad\ntype: index\nlibrary: [tools/, notes.md]\n---\n# Cad\n",
        );
        write(site, "justfile", "build:\n    true\n");
        write(site, "ui/src/main.ts", "x");
        write(site, "ui/package.json", "{}");
        write(site, "ui/node_modules/dep/index.js", "x");
        write(site, "ui/dist/index.html", "x");
        write(site, "ui/.env", "SECRET=1");
        write(site, "tools/run.py", "print()");
        write(site, "notes.md", "# notes");
        // Content: a work, tickets, a scratch folder.
        write(site, "moonlander/index.md", "---\ntype: work\n---\n");
        write(site, "tickets/0001.md", "# t");
        write(site, "tmp/out.png", "x");
        std::os::unix::fs::symlink("/etc", site.join("tools/etc")).unwrap();

        let definition = SiteDefinition::read(site).unwrap();
        let files: Vec<String> = definition
            .files()
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        assert_eq!(
            files,
            [
                "index.md",
                "justfile",
                "notes.md",
                "tools/run.py",
                "ui/package.json",
                "ui/src/main.ts",
            ]
        );
        let left_out: Vec<String> = definition
            .left_out()
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        assert_eq!(left_out, ["moonlander", "tickets", "tmp"]);
        assert!(definition.missing().is_empty());
    }

    #[test]
    fn extras_stay_inside_the_site() {
        for bad in ["../other", "/etc", "tools/../../x", ".git", "a/.hidden", ""] {
            let dir = tempfile::tempdir().unwrap();
            write(
                dir.path(),
                "index.md",
                &format!("---\nlibrary: [\"{bad}\"]\n---\n"),
            );
            assert!(SiteDefinition::read(dir.path()).is_err(), "{bad}");
        }
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "index.md", "---\nlibrary: gone/\n---\n");
        let definition = SiteDefinition::read(dir.path()).unwrap();
        assert_eq!(definition.missing(), [PathBuf::from("gone")]);
    }

    #[test]
    fn a_glob_takes_one_folders_matching_files_and_covers_only_them() {
        let dir = tempfile::tempdir().unwrap();
        let site = dir.path();
        write(
            site,
            "index.md",
            "---\nlibrary: [\"*.md\"]\n---\n# Agents\n",
        );
        write(site, "default.md", "# default");
        write(site, "printer.md", "# printer");
        write(site, "notes.txt", "x");
        write(site, "drafts/old.md", "# old");
        let definition = SiteDefinition::read(site).unwrap();
        let files: Vec<String> = definition
            .files()
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        assert_eq!(files, ["default.md", "index.md", "printer.md"]);
        let left_out: Vec<String> = definition
            .left_out()
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        assert_eq!(left_out, ["drafts", "notes.txt"]);
        assert!(definition.missing().is_empty());
        assert!(definition.covers(Path::new("gone.md")));
        assert!(definition.covers(Path::new("ui/src/main.ts")));
        assert!(!definition.covers(Path::new("drafts/old.md")));
        assert!(!definition.covers(Path::new("notes.txt")));
    }
}
