use std::collections::BTreeMap;

use crate::error::Result;
use crate::link::LinkTarget;
use crate::page::Page;
use crate::query::Query;
use crate::site_url::SiteUrl;
use crate::types::{IndexEntry, SearchHit};
use crate::workspace::Workspace;

/// In-memory index of all pages: frontmatter, titles, text and links.
///
/// Keyed by canonical URL (the actual file). Kept current by calling
/// [`Index::upsert`] / [`Index::remove`] from the file watcher.
#[derive(Debug, Default)]
pub struct Index {
    pages: BTreeMap<SiteUrl, IndexEntry>,
}

impl Index {
    /// Reads every page in the workspace. Unreadable files are skipped.
    pub fn build(workspace: &Workspace) -> Result<Self> {
        let mut index = Self::default();
        for path in workspace.all_pages() {
            let Some(url) = workspace.url_for_path(&path) else {
                continue;
            };
            if let Ok(page) = workspace.read_page(&url) {
                index.upsert(&page);
            }
        }
        Ok(index)
    }

    pub fn upsert(&mut self, page: &Page) {
        let links = page
            .links
            .iter()
            .filter_map(|l| match &l.target {
                LinkTarget::Site(url) => Some(url.without_fragment()),
                _ => None,
            })
            .collect();
        let entry = IndexEntry {
            url: page.url.clone(),
            title: page.title(),
            frontmatter: page.frontmatter.clone(),
            body_lower: page.body().to_lowercase(),
            links,
        };
        self.pages.insert(page.url.without_fragment(), entry);
    }

    pub fn remove(&mut self, url: &SiteUrl) {
        self.pages.remove(&url.without_fragment());
    }

    pub fn get(&self, url: &SiteUrl) -> Option<&IndexEntry> {
        self.canonicalize(url).and_then(|u| self.pages.get(&u))
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = &IndexEntry> {
        self.pages.values()
    }

    /// The indexed page `url` refers to (`site://plans/x` → `site://plans/x.md`).
    pub fn canonicalize(&self, url: &SiteUrl) -> Option<SiteUrl> {
        url.candidates()
            .iter()
            .filter_map(|c| SiteUrl::new(url.site(), c).ok())
            .find(|u| self.pages.contains_key(u))
    }

    /// Matching pages: title matches first, then by URL.
    pub fn search(&self, query: &Query) -> Vec<SearchHit> {
        let mut hits: Vec<(bool, &IndexEntry)> = self
            .pages
            .values()
            .filter(|e| query.matches(e))
            .map(|e| (!query.matches_title(e), e))
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.url.cmp(&b.1.url)));
        hits.into_iter()
            .map(|(_, e)| SearchHit {
                url: e.url.clone(),
                title: e.title.clone(),
            })
            .collect()
    }

    /// Pages that link to `url`.
    pub fn backlinks(&self, url: &SiteUrl) -> Vec<SiteUrl> {
        let Some(target) = self.canonicalize(url) else {
            return Vec::new();
        };
        self.pages
            .values()
            .filter(|e| {
                e.url != target
                    && e.links
                        .iter()
                        .any(|l| self.canonicalize(l).as_ref() == Some(&target))
            })
            .map(|e| e.url.clone())
            .collect()
    }

    /// Links from `url` that don't resolve to any page.
    pub fn broken_links(&self, url: &SiteUrl) -> Vec<SiteUrl> {
        self.get(url)
            .map(|e| {
                e.links
                    .iter()
                    .filter(|l| self.canonicalize(l).is_none())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}
