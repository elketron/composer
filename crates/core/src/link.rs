use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::site_url::SiteUrl;

/// A markdown link found in a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// The link text as shown to a reader.
    pub text: String,
    /// The destination as written in the markdown.
    pub href: String,
    pub target: LinkTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// A page in the workspace, resolved against the linking page.
    Site(SiteUrl),
    /// Anything with another scheme: `https:`, `mailto:`, ...
    External(String),
    /// A site link that could not be resolved (e.g. escapes its site).
    Invalid(String),
}

/// Extracts all links (not images) from `markdown`, resolving them relative to `base`.
pub fn extract_links(markdown: &str, base: &SiteUrl) -> Vec<Link> {
    let mut links = Vec::new();
    let mut open: Option<(String, String)> = None;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                open = Some((dest_url.to_string(), String::new()))
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, buf)) = &mut open {
                    buf.push_str(&text);
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some((href, text)) = open.take() {
                    let target = resolve(&href, base);
                    links.push(Link { text, href, target });
                }
            }
            _ => {}
        }
    }
    links
}

fn resolve(href: &str, base: &SiteUrl) -> LinkTarget {
    if has_foreign_scheme(href) {
        return LinkTarget::External(href.to_string());
    }
    match base.join(href) {
        Ok(url) => LinkTarget::Site(url),
        Err(e) => LinkTarget::Invalid(e.to_string()),
    }
}

/// True for `scheme:` prefixes other than `site:`.
fn has_foreign_scheme(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid && scheme != "site"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_resolves() {
        let base = SiteUrl::parse("site://memory/index.md").unwrap();
        let md = "See [user](user.md), [plan](site://plans/composer), \
                  [`code` docs](https://example.com), [pages](#pages) and ![img](x.png).";
        let links = extract_links(md, &base);
        let shown: Vec<_> = links
            .iter()
            .map(|l| match &l.target {
                LinkTarget::Site(u) => format!("{}={u}", l.text),
                LinkTarget::External(u) => format!("{}=ext:{u}", l.text),
                LinkTarget::Invalid(_) => format!("{}=invalid", l.text),
            })
            .collect();
        assert_eq!(
            shown,
            [
                "user=site://memory/user.md",
                "plan=site://plans/composer",
                "code docs=ext:https://example.com",
                "pages=site://memory/index.md#pages",
            ]
        );
    }

    #[test]
    fn invalid_links_are_kept() {
        let base = SiteUrl::parse("site://memory/user.md").unwrap();
        let links = extract_links("[x](../../etc/passwd)", &base);
        assert!(matches!(links[0].target, LinkTarget::Invalid(_)));
    }
}
