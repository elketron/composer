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

    /// One line about the page: frontmatter `description`, else the first sentence of its
    /// first paragraph (markdown links and emphasis removed), at most about 100 characters.
    pub fn description(&self) -> String {
        if let Some(d) = self
            .frontmatter
            .get_str("description")
            .filter(|d| !d.trim().is_empty())
        {
            return d.trim().to_string();
        }
        let Some(line) = self.body().lines().map(str::trim).find(|l| {
            !l.is_empty()
                && !l.starts_with(['#', '|', '-', '*', '>', '`', '<', '!'])
                && !l.starts_with("1.")
        }) else {
            return String::new();
        };
        let plain = strip_inline_markdown(line);
        let sentence = match plain.find(". ") {
            Some(end) => &plain[..=end],
            None => plain.as_str(),
        };
        if sentence.chars().count() <= 100 {
            return sentence.trim().to_string();
        }
        let cut: String = sentence.chars().take(99).collect();
        format!("{}…", cut.trim_end())
    }
}

/// `[text](url)` → `text`; `**`, `__`, `` ` `` dropped.
fn strip_inline_markdown(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "").replace('`', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, raw: &str) -> Page {
        Page::parse(SiteUrl::parse(url).unwrap(), raw.to_string())
    }

    #[test]
    fn descriptions() {
        let described = page(
            "site://a/index.md",
            "---\ndescription: Said so\n---\n# A\nBody.\n",
        );
        assert_eq!(described.description(), "Said so");
        let body = page(
            "site://a/index.md",
            "# A\n\n| table |\n\nWhat **you** know: see [user](user.md). More text.\n",
        );
        assert_eq!(body.description(), "What you know: see user.");
        let long = page("site://a/b.md", &format!("{}\n", "word ".repeat(40)));
        assert!(long.description().ends_with('…') && long.description().chars().count() <= 100);
        assert_eq!(
            page("site://a/c.md", "# Only a heading\n").description(),
            ""
        );
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
