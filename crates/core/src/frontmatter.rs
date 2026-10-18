use serde_json::{Map, Value};

use crate::error::{CoreError, Result};

/// The YAML block at the top of a page, between `---` lines.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frontmatter(Map<String, Value>);

impl Frontmatter {
    /// Splits a page into its raw frontmatter YAML (if any) and the body.
    pub fn split(raw: &str) -> (Option<&str>, &str) {
        let Some(after_open) = raw
            .strip_prefix("---\n")
            .or_else(|| raw.strip_prefix("---\r\n"))
        else {
            return (None, raw);
        };
        let mut offset = 0;
        for line in after_open.split_inclusive('\n') {
            if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
                let yaml = &after_open[..offset];
                let body = &after_open[offset + line.len()..];
                return (Some(yaml), body);
            }
            offset += line.len();
        }
        (None, raw)
    }

    /// Parses frontmatter YAML. Must be a mapping (or empty).
    pub fn parse(yaml: &str) -> Result<Self> {
        if yaml.trim().is_empty() {
            return Ok(Self::default());
        }
        match serde_norway::from_str::<Value>(yaml) {
            Ok(Value::Object(map)) => Ok(Self(map)),
            Ok(Value::Null) => Ok(Self::default()),
            Ok(_) => Err(CoreError::InvalidFrontmatter(
                "expected key: value pairs".into(),
            )),
            Err(e) => Err(CoreError::InvalidFrontmatter(e.to_string())),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// A scalar value as a string (strings, numbers, bools).
    pub fn get_str(&self, key: &str) -> Option<String> {
        self.get(key).and_then(scalar_to_string)
    }

    pub fn title(&self) -> Option<String> {
        self.get_str("title")
    }

    /// Case-insensitive match of `key` against `value`.
    /// For lists (e.g. `tags`), matches if any element equals `value`.
    pub fn matches(&self, key: &str, value: &str) -> bool {
        match self.get(key) {
            Some(Value::Array(items)) => items.iter().any(|v| scalar_eq(v, value)),
            Some(v) => scalar_eq(v, value),
            None => false,
        }
    }

    pub fn as_map(&self) -> &Map<String, Value> {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn scalar_eq(value: &Value, expected: &str) -> bool {
    scalar_to_string(value).is_some_and(|s| s.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str =
        "---\ntitle: Server API\ntype: plan\ntags: [api, Rust]\nid: \"0003\"\n---\n# Server API\n";

    #[test]
    fn splits_frontmatter_from_body() {
        let (yaml, body) = Frontmatter::split(PAGE);
        assert!(yaml.unwrap().starts_with("title: Server API"));
        assert_eq!(body, "# Server API\n");
    }

    #[test]
    fn no_frontmatter() {
        assert_eq!(Frontmatter::split("# hi\n"), (None, "# hi\n"));
        // an opening fence without a closing one is body text
        assert_eq!(
            Frontmatter::split("---\ntitle: x\n"),
            (None, "---\ntitle: x\n")
        );
    }

    #[test]
    fn handles_crlf() {
        let (yaml, body) = Frontmatter::split("---\r\ntitle: x\r\n---\r\nbody");
        assert_eq!(yaml, Some("title: x\r\n"));
        assert_eq!(body, "body");
    }

    #[test]
    fn parses_and_matches() {
        let fm = Frontmatter::parse(Frontmatter::split(PAGE).0.unwrap()).unwrap();
        assert_eq!(fm.title().as_deref(), Some("Server API"));
        assert!(fm.matches("type", "PLAN"));
        assert!(fm.matches("tags", "rust"));
        assert!(fm.matches("id", "0003"));
        assert!(!fm.matches("status", "open"));
    }

    #[test]
    fn rejects_non_mapping() {
        assert!(Frontmatter::parse("- a\n- b\n").is_err());
        assert!(Frontmatter::parse("title: [unclosed\n").is_err());
        assert!(Frontmatter::parse("").unwrap().is_empty());
    }
}
