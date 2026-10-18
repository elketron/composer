use crate::frontmatter::Frontmatter;
use crate::link::{Link, extract_links};
use crate::site_url::SiteUrl;

/// A parsed markdown page.
#[derive(Debug, Clone)]
pub struct Page {
    /// Canonical URL: points at the actual file (e.g. `site://memory/index.md`).
    pub url: SiteUrl,
    /// The full file contents, as the agent sees it.
    pub raw: String,
    pub frontmatter: Frontmatter,
    /// Set when the frontmatter is present but invalid; the page is still usable.
    pub frontmatter_error: Option<String>,
    /// Byte offset in `raw` where the body (after frontmatter) starts.
    body_start: usize,
    pub links: Vec<Link>,
}

impl Page {
    pub fn parse(url: SiteUrl, raw: String) -> Self {
        let (yaml, body) = Frontmatter::split(&raw);
        let body_start = raw.len() - body.len();
        let (frontmatter, frontmatter_error) = match yaml.map(Frontmatter::parse) {
            None => (Frontmatter::default(), None),
            Some(Ok(fm)) => (fm, None),
            Some(Err(e)) => (Frontmatter::default(), Some(e.to_string())),
        };
        let links = extract_links(body, &url);
        Self {
            url,
            raw,
            frontmatter,
            frontmatter_error,
            body_start,
            links,
        }
    }

    /// The markdown after the frontmatter.
    pub fn body(&self) -> &str {
        &self.raw[self.body_start..]
    }

    /// Frontmatter `title`, else the first `# ` heading, else the file name
    /// (for an `index.md`: its folder, or the site name).
    pub fn title(&self) -> String {
        if let Some(title) = self.frontmatter.title() {
            return title;
        }
        if let Some(heading) = self.body().lines().find_map(|l| l.strip_prefix("# ")) {
            return heading.trim().to_string();
        }
        let mut segments = self.url.path().rsplit('/').filter(|s| !s.is_empty());
        match segments.next() {
            None | Some("index.md") => segments.next().unwrap_or(self.url.site()).to_string(),
            Some(file) => file.strip_suffix(".md").unwrap_or(file).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, raw: &str) -> Page {
        Page::parse(SiteUrl::parse(url).unwrap(), raw.to_string())
    }

    #[test]
    fn title_fallbacks() {
        assert_eq!(
            page("site://a/x.md", "---\ntitle: T\n---\n# H\n").title(),
            "T"
        );
        assert_eq!(page("site://a/x.md", "# Heading\ntext").title(), "Heading");
        assert_eq!(page("site://a/notes/x.md", "text").title(), "x");
        assert_eq!(page("site://memory/index.md", "text").title(), "memory");
        assert_eq!(
            page("site://memory/projects/index.md", "text").title(),
            "projects"
        );
    }

    #[test]
    fn body_and_links_skip_frontmatter() {
        let p = page("site://a/x.md", "---\ntitle: T\n---\nsee [b](b.md)\n");
        assert_eq!(p.body(), "see [b](b.md)\n");
        assert_eq!(p.links.len(), 1);
    }

    #[test]
    fn invalid_frontmatter_keeps_page() {
        let p = page("site://a/x.md", "---\ntitle: [oops\n---\n# Still here\n");
        assert!(p.frontmatter_error.is_some());
        assert_eq!(p.title(), "Still here");
    }
}
