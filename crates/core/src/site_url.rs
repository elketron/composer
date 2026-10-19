use std::fmt;

use crate::error::{CoreError, Result};

const SCHEME: &str = "site://";

/// A `site://<site>/<path>#<fragment>` address.
///
/// The path is normalized (no `.`, `..` or empty segments) and can never point
/// outside its site. An empty path is the site's homepage. The URL is purely
/// syntactic: `site://plans/x` may name `x.md` or `x/index.md`; [`SiteUrl::candidates`]
/// lists the files it may resolve to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SiteUrl {
    site: String,
    path: String,
    fragment: Option<String>,
}

impl SiteUrl {
    /// Parses an absolute `site://` URL.
    pub fn parse(url: &str) -> Result<Self> {
        let invalid = |reason| CoreError::InvalidUrl {
            url: url.to_string(),
            reason,
        };
        let rest = url
            .strip_prefix(SCHEME)
            .ok_or_else(|| invalid("must start with site://"))?;
        let (rest, fragment) = split_fragment(rest);
        let (site, path) = rest.split_once('/').unwrap_or((rest, ""));
        if !is_valid_site_name(site) {
            return Err(invalid(
                "site name must be letters, digits, `-`, `_` or `.`, not starting with `.`",
            ));
        }
        let path = normalize(&[], path).ok_or_else(|| invalid("path escapes the site"))?;
        Ok(Self {
            site: site.to_string(),
            path,
            fragment,
        })
    }

    /// Builds a URL from a site name and a path relative to the site.
    pub fn new(site: &str, path: &str) -> Result<Self> {
        Self::parse(&format!("{SCHEME}{site}/{path}"))
    }

    /// Resolves `href` as found in a link on this page.
    ///
    /// Handles absolute `site://` URLs, fragment-only links (`#x`) and paths
    /// relative to this page's directory. External URLs are not handled here;
    /// see [`crate::extract_links`].
    pub fn join(&self, href: &str) -> Result<Self> {
        if href.starts_with(SCHEME) {
            return Self::parse(href);
        }
        let (rel, fragment) = split_fragment(href);
        if rel.is_empty() {
            return Ok(Self {
                fragment,
                ..self.clone()
            });
        }
        let invalid = |reason| CoreError::InvalidUrl {
            url: href.to_string(),
            reason,
        };
        let base: Vec<&str> = if rel.starts_with('/') {
            Vec::new()
        } else {
            let mut dir: Vec<&str> = self.path.split('/').filter(|s| !s.is_empty()).collect();
            dir.pop();
            dir
        };
        let path = normalize(&base, rel).ok_or_else(|| invalid("path escapes the site"))?;
        Ok(Self {
            site: self.site.clone(),
            path,
            fragment,
        })
    }

    pub fn site(&self) -> &str {
        &self.site
    }

    /// Path inside the site, without a leading slash. Empty for the homepage.
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// The same URL without its fragment.
    pub fn without_fragment(&self) -> Self {
        Self {
            fragment: None,
            ..self.clone()
        }
    }

    /// Files (relative to the site directory) this URL may name, in lookup order.
    pub fn candidates(&self) -> Vec<String> {
        let path = self.path.as_str();
        if path.is_empty() {
            return vec!["index.md".to_string()];
        }
        let last = path.rsplit('/').next().unwrap_or(path);
        // A site's actions file has no extension and is no page.
        if last.contains('.') || last == "justfile" {
            vec![path.to_string()]
        } else {
            vec![
                format!("{path}.md"),
                format!("{path}/index.md"),
                path.to_string(),
            ]
        }
    }

    /// The file a new page at this URL is written to.
    pub fn file_for_write(&self) -> String {
        self.candidates().remove(0)
    }
}

impl fmt::Display for SiteUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}{}", self.site)?;
        if !self.path.is_empty() {
            write!(f, "/{}", self.path)?;
        }
        if let Some(fragment) = &self.fragment {
            write!(f, "#{fragment}")?;
        }
        Ok(())
    }
}

fn split_fragment(s: &str) -> (&str, Option<String>) {
    match s.split_once('#') {
        Some((rest, frag)) if !frag.is_empty() => (rest, Some(frag.to_string())),
        Some((rest, _)) => (rest, None),
        None => (s, None),
    }
}

fn is_valid_site_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Joins `rel` onto `base` segments, resolving `.` and `..`.
/// Returns `None` if the result would leave the site, or contains a backslash or NUL.
fn normalize(base: &[&str], rel: &str) -> Option<String> {
    if rel.contains(['\\', '\0']) {
        return None;
    }
    let mut segments: Vec<&str> = base.to_vec();
    for segment in rel.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            s => segments.push(s),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> SiteUrl {
        SiteUrl::parse(s).unwrap()
    }

    #[test]
    fn parses_and_displays() {
        let u = url("site://memory/projects/composer.md#decisions");
        assert_eq!(u.site(), "memory");
        assert_eq!(u.path(), "projects/composer.md");
        assert_eq!(u.fragment(), Some("decisions"));
        assert_eq!(
            u.to_string(),
            "site://memory/projects/composer.md#decisions"
        );
    }

    #[test]
    fn site_root_is_homepage() {
        assert_eq!(url("site://memory").path(), "");
        assert_eq!(url("site://memory/").to_string(), "site://memory");
        assert_eq!(url("site://memory").candidates(), ["index.md"]);
        assert_eq!(url("site://plans/justfile").file_for_write(), "justfile");
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(url("site://a/x/./y//z.md").path(), "x/y/z.md");
        assert_eq!(url("site://a/x/../y.md").path(), "y.md");
    }

    #[test]
    fn rejects_bad_urls() {
        assert!(SiteUrl::parse("http://x").is_err());
        assert!(SiteUrl::parse("site://").is_err());
        assert!(SiteUrl::parse("site://.hidden/x").is_err());
        assert!(SiteUrl::parse("site://a/../b/x.md").is_err());
        assert!(SiteUrl::parse("site://a/x\\y").is_err());
    }

    #[test]
    fn joins_relative_links() {
        let page = url("site://memory/projects/composer.md");
        assert_eq!(
            page.join("other.md").unwrap().to_string(),
            "site://memory/projects/other.md"
        );
        assert_eq!(
            page.join("../user.md").unwrap().to_string(),
            "site://memory/user.md"
        );
        assert_eq!(
            page.join("/topics/x.md").unwrap().to_string(),
            "site://memory/topics/x.md"
        );
        assert_eq!(
            page.join("#done").unwrap().to_string(),
            "site://memory/projects/composer.md#done"
        );
        assert_eq!(
            page.join("site://plans/x").unwrap().to_string(),
            "site://plans/x"
        );
        assert!(page.join("../../../etc/passwd").is_err());
    }

    #[test]
    fn joins_from_homepage() {
        let home = url("site://memory");
        assert_eq!(
            home.join("user.md").unwrap().to_string(),
            "site://memory/user.md"
        );
    }

    #[test]
    fn candidates_for_extensionless_paths() {
        assert_eq!(
            url("site://plans/server-api").candidates(),
            ["server-api.md", "server-api/index.md", "server-api"]
        );
        assert_eq!(
            url("site://design/mockup.json").candidates(),
            ["mockup.json"]
        );
    }
}
