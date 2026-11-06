use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use composer_core::{Page, SiteUrl};
use serde_json::Value;

/// The part of a site that is its definition, and so what the library holds, installs and
/// publishes: `index.md`, `justfile`, `ui/` (source only), `stages/`, `_config/`, `templates/`, and
/// every path the index lists under `library:` (`library: [tools/, comfy-workflows/]`).
///
/// Never in it: pipeline works, tickets and other pages outside those paths; `node_modules/` and
/// `dist/` anywhere; hidden files; symlinks (not followed, not copied). A `library:` entry that
/// leaves the site is refused, so a definition can't carry anything from outside its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteDefinition {
    /// Files, relative to the site folder.
    files: BTreeSet<PathBuf>,
    /// Top-level entries of the site that are not part of it (works, tickets, pages, ...).
    left_out: Vec<PathBuf>,
    /// `library:` entries that don't exist.
    missing: Vec<PathBuf>,
}

/// Always part of a definition, when present.
const CORE: [&str; 6] = [
    "index.md",
    "justfile",
    "ui",
    "stages",
    "_config",
    "templates",
];
/// Never part of one, at any depth.
const NEVER: [&str; 2] = ["node_modules", "dist"];

impl SiteDefinition {
    /// The definition of the site in `dir`.
    pub fn read(dir: &Path) -> Result<Self, String> {
        let mut roots: BTreeSet<PathBuf> = CORE.iter().map(PathBuf::from).collect();
        roots.extend(extras(dir)?);
        let mut files = BTreeSet::new();
        let mut missing = Vec::new();
        for root in &roots {
            let path = dir.join(root);
            match std::fs::symlink_metadata(&path) {
                Ok(_) => collect(dir, &path, &mut files),
                Err(_) if !CORE.iter().any(|c| Path::new(c) == root) => missing.push(root.clone()),
                Err(_) => {}
            }
        }
        let left_out = top_level(dir)
            .into_iter()
            .filter(|name| !roots.iter().any(|root| root.starts_with(name)))
            .collect();
        Ok(Self {
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
}

/// The index's `library:` entries, each a relative path inside the site.
fn extras(dir: &Path) -> Result<Vec<PathBuf>, String> {
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
    entries.iter().map(|entry| inside(entry)).collect()
}

/// `entry` as a path inside the site; refused when it leaves it or names a hidden file.
fn inside(entry: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(entry.trim().trim_end_matches('/'));
    let plain = path.components().all(|c| match c {
        Component::Normal(part) => !part.to_string_lossy().starts_with('.'),
        _ => false,
    });
    if path.as_os_str().is_empty() || !plain {
        return Err(format!(
            "`library: {entry}` is not a path inside the site: list folders or files of the site \
             itself (`tools/`, `comfy-workflows/`), without `..`, `/` at the start or hidden names"
        ));
    }
    Ok(path)
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

/// The site's top-level entries (not hidden).
fn top_level(dir: &Path) -> Vec<PathBuf> {
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| PathBuf::from(e.file_name()))
        .filter(|n| !n.to_string_lossy().starts_with('.'))
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
            "---\ntitle: Cad\ntype: pipeline\nlibrary: [tools/, notes.md]\n---\n# Cad\n",
        );
        write(site, "justfile", "build:\n    true\n");
        write(site, "ui/src/main.ts", "x");
        write(site, "ui/package.json", "{}");
        write(site, "ui/node_modules/dep/index.js", "x");
        write(site, "ui/dist/index.html", "x");
        write(site, "ui/.env", "SECRET=1");
        write(site, "stages/01_plan.md", "# plan");
        write(site, "_config/defaults.toml", "");
        write(site, "tools/run.py", "print()");
        write(site, "notes.md", "# notes");
        // Content: a pipeline work, tickets, a scratch folder.
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
                "_config/defaults.toml",
                "index.md",
                "justfile",
                "notes.md",
                "stages/01_plan.md",
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
}
