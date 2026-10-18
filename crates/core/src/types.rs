//! Plain data types without behaviour.

use crate::frontmatter::Frontmatter;
use crate::site_url::SiteUrl;

/// A top-level folder in the workspace.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteInfo {
    pub name: String,
    /// From the site's `index.md`, falling back to the name.
    pub title: String,
    pub has_index: bool,
    pub has_justfile: bool,
    pub has_ui: bool,
}

/// What the index keeps per page.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub url: SiteUrl,
    pub title: String,
    pub frontmatter: Frontmatter,
    /// Lowercased body, for full-text matching.
    pub body_lower: String,
    /// Outgoing site links, without fragments, as written (not yet canonical).
    pub links: Vec<SiteUrl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub url: SiteUrl,
    pub title: String,
}
