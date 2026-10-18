use crate::types::IndexEntry;

/// A search query: `key:value` filters plus free-text terms.
///
/// `site:x` matches the site name; any other key matches frontmatter
/// (`type:ticket`, `status:open`, `tags:rust`). Terms match the title or body,
/// case-insensitively. Everything must match. Quote to keep spaces: `"device pairing"`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub filters: Vec<(String, String)>,
    pub terms: Vec<String>,
}

impl Query {
    pub fn parse(input: &str) -> Self {
        let mut query = Self::default();
        for token in tokenize(input) {
            match as_filter(&token) {
                Some((key, value)) => query
                    .filters
                    .push((key.to_ascii_lowercase(), value.to_string())),
                None => query.terms.push(token.to_lowercase()),
            }
        }
        query
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty() && self.terms.is_empty()
    }

    pub fn matches(&self, entry: &IndexEntry) -> bool {
        let filters_match = self.filters.iter().all(|(key, value)| match key.as_str() {
            "site" => entry.url.site().eq_ignore_ascii_case(value),
            _ => entry.frontmatter.matches(key, value),
        });
        filters_match && {
            let title = entry.title.to_lowercase();
            self.terms
                .iter()
                .all(|t| title.contains(t.as_str()) || entry.body_lower.contains(t.as_str()))
        }
    }

    /// True if every term appears in the title; used to rank title hits first.
    pub fn matches_title(&self, entry: &IndexEntry) -> bool {
        let title = entry.title.to_lowercase();
        !self.terms.is_empty() && self.terms.iter().all(|t| title.contains(t.as_str()))
    }
}

/// `key:value` where key is `[a-z_]+` and value is non-empty. URLs (`site://x`) are not filters.
fn as_filter(token: &str) -> Option<(&str, &str)> {
    let (key, value) = token.split_once(':')?;
    let key_ok = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic() || c == '_');
    (key_ok && !value.is_empty() && !value.starts_with("//")).then_some((key, value))
}

/// Splits on whitespace; double quotes group words (also inside `key:"a b"`).
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in input.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_filters_and_terms() {
        let q = Query::parse(r#"type:ticket Status:open "device pairing" QR site://x"#);
        assert_eq!(
            q.filters,
            [
                ("type".into(), "ticket".into()),
                ("status".into(), "open".into())
            ]
        );
        assert_eq!(q.terms, ["device pairing", "qr", "site://x"]);
    }

    #[test]
    fn quoted_filter_values() {
        let q = Query::parse(r#"title:"server api""#);
        assert_eq!(q.filters, [("title".into(), "server api".into())]);
    }

    #[test]
    fn empty() {
        assert!(Query::parse("   ").is_empty());
    }
}
