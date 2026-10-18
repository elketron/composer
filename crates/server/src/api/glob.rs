/// Matches a URL against a subscription pattern:
/// `*` or `**` (everything), `site://x/**` (anything below), `site://x/*` (one level), or exact.
pub fn url_matches(pattern: &str, url: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return url == prefix || url.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return url
            .strip_prefix(&format!("{prefix}/"))
            .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'));
    }
    pattern == url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(url_matches(
            "site://design/**",
            "site://design/mockups/login/mockup.json"
        ));
        assert!(url_matches("site://design/*", "site://design/index.md"));
        assert!(!url_matches(
            "site://design/*",
            "site://design/mockups/x.md"
        ));
        assert!(!url_matches("site://design/**", "site://designs/x.md"));
        assert!(url_matches("site://a/x.md", "site://a/x.md"));
        assert!(url_matches("*", "site://a/x.md"));
    }
}
