//! A project as one value: the page, the folder, the sites and the tags.
//!
//! The page is `site://projects/<name>.md` (`type: project`, `root:`); the old
//! `site://memory/projects/<name>.md` (`type: memory`, `kind: project`, `path:`) still reads as a
//! fallback. A name or a path resolves to the folder first, then to the page whose `root:` is that
//! folder. Every spelling gives the same canonical root.

use std::path::PathBuf;

use composer_core::{Page, SiteUrl};
use serde_json::Value;

use crate::project::ProjectPath;
use crate::server::Server;

/// One project, resolved from a page, a name or a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    /// The folder's name.
    pub name: String,
    /// `site://projects/<name>.md`, when there is a page.
    pub page: Option<SiteUrl>,
    /// Canonical, existing, a directory.
    pub root: PathBuf,
    /// The page's `sites:` (or the old `workflow:`): what operates on it.
    pub sites: Vec<String>,
    pub tags: Vec<String>,
}

impl ProjectRef {
    /// A page URL, a name in `[projects] dir`, or a path (`~/...`, absolute, relative).
    pub fn resolve(server: &Server, spec: &str) -> Result<Self, String> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err("a project is a page, a name, or a path".into());
        }
        let projects_dir = server.config().projects.dir.clone();
        if let Ok(url) = SiteUrl::parse(spec) {
            let page = server.web.read_page(&url).map_err(|e| e.to_string())?;
            return Self::from_page(&page, &projects_dir)
                .ok_or_else(|| format!("{url} is not a project page (type: project, root:)"));
        }
        let root = ProjectPath::resolve(spec, &projects_dir)?;
        if let Some(url) = Self::find_page(server, &root)
            && let Ok(page) = server.web.read_page(&url)
            && let Some(project) = Self::from_page(&page, &projects_dir)
        {
            return Ok(project);
        }
        Ok(Self {
            name: folder_name(&root),
            page: None,
            root: root.as_path().to_path_buf(),
            sites: Vec::new(),
            tags: Vec::new(),
        })
    }

    /// The page URL as a string, for the protocol.
    pub fn page_url(&self) -> Option<String> {
        self.page.as_ref().map(ToString::to_string)
    }

    /// A project page (`type: project`, or the old `type: memory` + `kind: project`).
    pub(crate) fn from_page(page: &Page, projects_dir: &str) -> Option<Self> {
        let frontmatter = &page.frontmatter;
        let kind = frontmatter.get_str("type").unwrap_or_default();
        let is_project =
            kind == "project" || (kind == "memory" && frontmatter.matches("kind", "project"));
        if !is_project {
            return None;
        }
        let root = frontmatter
            .get_str("root")
            .or_else(|| frontmatter.get_str("path"))?;
        let root = ProjectPath::resolve(&root, projects_dir).ok()?;
        let mut sites = strings(frontmatter.get("sites"));
        if sites.is_empty() {
            sites = strings(frontmatter.get("workflow"));
        }
        Some(Self {
            name: folder_name(&root),
            page: Some(page.url.clone()),
            root: root.as_path().to_path_buf(),
            sites,
            tags: strings(frontmatter.get("tags")),
        })
    }

    /// The project page whose `root:` (or old `path:`) is `root`.
    fn find_page(server: &Server, root: &ProjectPath) -> Option<SiteUrl> {
        let projects_dir = server.config().projects.dir.clone();
        let index = server.index.read().expect("index lock");
        index
            .entries()
            .find(|entry| {
                let frontmatter = &entry.frontmatter;
                let kind = frontmatter.get_str("type").unwrap_or_default();
                let is_project = kind == "project"
                    || (kind == "memory" && frontmatter.matches("kind", "project"));
                if !is_project {
                    return false;
                }
                frontmatter
                    .get_str("root")
                    .or_else(|| frontmatter.get_str("path"))
                    .and_then(|root| ProjectPath::resolve(&root, &projects_dir).ok())
                    .is_some_and(|page_root| page_root == *root)
            })
            .map(|entry| entry.url.clone())
    }
}

fn folder_name(root: &ProjectPath) -> String {
    root.as_path()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(dir: &std::path::Path, name: &str) -> ProjectPath {
        let path = dir.join(name);
        std::fs::create_dir_all(&path).unwrap();
        ProjectPath::existing(&path).unwrap()
    }

    fn page(url: &str, raw: &str) -> Page {
        Page::parse(SiteUrl::parse(url).unwrap(), raw.to_string())
    }

    #[test]
    fn a_project_page_reads_the_folder_and_the_sites() {
        let dir = tempfile::tempdir().unwrap();
        let path = root(dir.path(), "A");
        let root = path.as_path().display().to_string();
        let page = page(
            "site://projects/A.md",
            &format!(
                "---\ntitle: A\ntype: project\nroot: {root}\nsites: [coding, video]\ntags: [game]\n---\n# A\n"
            ),
        );
        let project = ProjectRef::from_page(&page, dir.path().to_str().unwrap()).unwrap();
        assert_eq!(project.name, "A");
        assert_eq!(project.root, path.as_path().to_path_buf());
        assert_eq!(project.sites, ["coding", "video"]);
        assert_eq!(project.tags, ["game"]);
        assert_eq!(project.page_url().as_deref(), Some("site://projects/A.md"));
    }

    #[test]
    fn the_old_memory_page_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = root(dir.path(), "A");
        let root = path.as_path().display().to_string();
        let page = page(
            "site://memory/projects/A.md",
            &format!(
                "---\ntitle: A\ntype: memory\nkind: project\npath: {root}\nworkflow: coding\n---\n# A\n"
            ),
        );
        let project = ProjectRef::from_page(&page, dir.path().to_str().unwrap()).unwrap();
        assert_eq!(project.root, path.as_path().to_path_buf());
        assert_eq!(project.sites, ["coding"]);
        assert_eq!(
            project.page_url().as_deref(),
            Some("site://memory/projects/A.md")
        );
    }

    #[test]
    fn a_page_that_is_not_a_project_is_refused_by_the_resolver() {
        let dir = tempfile::tempdir().unwrap();
        let page = page("site://plans/A", "---\ntitle: A\ntype: plan\n---\n# A\n");
        assert!(ProjectRef::from_page(&page, dir.path().to_str().unwrap()).is_none());
    }
}
