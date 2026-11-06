use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{CoreError, Result};
use crate::mounts::Mounts;
use crate::page::Page;
use crate::site_url::SiteUrl;
use crate::types::SiteInfo;

/// The folder holding all sites (`~/.composer/sites`).
#[derive(Debug, Clone)]
pub struct Web {
    root: PathBuf,
    mounts: Mounts,
}

impl Web {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            mounts: Mounts::new(root.clone()),
            root,
        }
    }

    /// Plus `allowed`: roots a symlink inside a site may point into (scoped mounts,
    /// ticket 0122: the projects directory from config); the web itself stays allowed.
    pub fn with_allowed(root: impl Into<PathBuf>, allowed: Vec<PathBuf>) -> Self {
        let root = root.into();
        Self {
            mounts: Mounts::with_allowed(root.clone(), allowed),
            root,
        }
    }

    /// Swap the allowed roots a mount may point into (config reload).
    pub fn set_allowed(&self, allowed: Vec<PathBuf>) {
        self.mounts.set_allowed(Some(allowed));
    }

    /// The mount points there are now (symlinks into an allowed root), as web paths.
    pub fn mount_points(&self) -> Vec<PathBuf> {
        self.mounts.current()
    }

    /// Whether the web may read, list, write or watch `path` (its own address for it).
    pub fn allows(&self, path: &Path) -> bool {
        self.mounts.allows(path)
    }

    /// The same check as an error, naming the mount when a symlink is refused.
    /// Public so the server's file routes guard their own reads the same way.
    pub fn check(&self, path: &Path) -> Result<()> {
        self.mounts.check(path)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// All sites, sorted by name. Hidden folders are skipped.
    pub fn sites(&self) -> Result<Vec<SiteInfo>> {
        let mut sites = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|e| io(&self.root, e))? {
            let entry = entry.map_err(|e| io(&self.root, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.path().is_dir() || SiteUrl::new(&name, "").is_err() {
                continue;
            }
            let dir = entry.path();
            let index = SiteUrl::new(&name, "")?;
            let page = self.read_page(&index).ok();
            let title = page.as_ref().map_or_else(|| name.clone(), |p| p.title());
            let description = page.as_ref().map(|p| p.description()).unwrap_or_default();
            sites.push(SiteInfo {
                title,
                has_index: dir.join("index.md").is_file(),
                has_justfile: dir.join("justfile").is_file(),
                has_ui: dir.join("ui").is_dir(),
                app_built: dir.join("ui/dist/index.html").is_file(),
                description,
                name,
            });
        }
        sites.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(sites)
    }

    /// The existing file `url` refers to.
    pub fn locate(&self, url: &SiteUrl) -> Result<PathBuf> {
        let site_dir = self.site_dir(url)?;
        let path = url
            .candidates()
            .into_iter()
            .map(|c| site_dir.join(c))
            .find(|p| p.is_file())
            .ok_or_else(|| CoreError::PageNotFound(url.to_string()))?;
        self.check(&path)?;
        Ok(path)
    }

    pub fn read_page(&self, url: &SiteUrl) -> Result<Page> {
        let path = self.locate(url)?;
        let raw = fs::read_to_string(&path).map_err(|e| io(&path, e))?;
        let canonical = self
            .url_for_path(&path)
            .ok_or_else(|| CoreError::PageNotFound(url.to_string()))?;
        Ok(Page::parse(canonical, raw))
    }

    /// Writes `content` to the page at `url`, creating folders as needed.
    /// An existing file is overwritten; otherwise extensionless URLs get `.md`.
    /// Returns the canonical URL of the written file.
    pub fn write_page(&self, url: &SiteUrl, content: &str) -> Result<SiteUrl> {
        self.write_file(url, content.as_bytes())
    }

    /// Like [`Web::write_page`], for any file (JSON, images, ...).
    pub fn write_file(&self, url: &SiteUrl, content: &[u8]) -> Result<SiteUrl> {
        let path = match self.locate(url) {
            Ok(path) => path,
            Err(CoreError::PageNotFound(_)) => {
                self.root.join(url.site()).join(url.file_for_write())
            }
            Err(e) => return Err(e),
        };
        self.check(&path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        fs::write(&path, content).map_err(|e| io(&path, e))?;
        self.url_for_path(&path)
            .ok_or_else(|| CoreError::PageNotFound(url.to_string()))
    }

    /// The canonical URL for a file inside the workspace.
    pub fn url_for_path(&self, path: &Path) -> Option<SiteUrl> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let mut parts = rel.iter().map(|p| p.to_str());
        let site = parts.next()??;
        let rest: Option<Vec<&str>> = parts.collect();
        SiteUrl::new(site, &rest?.join("/")).ok()
    }

    /// Every markdown page in the workspace.
    /// Skips hidden folders, `node_modules`, and each site's `ui/` folder.
    /// With an allowed list set, follows symlinks that land inside the web or an allowed
    /// root (scoped mounts); a mount pointing elsewhere contributes no pages. Without an
    /// allowed list, walks do not descend into symlinks, as before the feature.
    pub fn all_pages(&self) -> impl Iterator<Item = PathBuf> + '_ {
        let follow = self.mounts.follows_links();
        WalkDir::new(&self.root)
            .min_depth(1)
            .follow_links(follow)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !(name.starts_with('.')
                    || name == "node_modules"
                    || (e.depth() == 2 && name == "ui" && e.file_type().is_dir()))
                    && self.allows(e.path())
            })
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.depth() >= 2
                    && e.file_type().is_file()
                    && e.path().extension().is_some_and(|x| x == "md")
            })
            .map(|e| e.into_path())
    }

    fn site_dir(&self, url: &SiteUrl) -> Result<PathBuf> {
        let dir = self.root.join(url.site());
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(CoreError::SiteNotFound(url.site().to_string()))
        }
    }
}

fn io(path: &Path, source: std::io::Error) -> CoreError {
    CoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}
