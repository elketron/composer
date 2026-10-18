use serde_json::Value;

/// Which sites an agent may use, from its `sites:` frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteAccess {
    All,
    Only(Vec<String>),
}

/// Sites every agent can read: the manual and agent definitions.
const ALWAYS_READABLE: [&str; 2] = ["home", "agents"];

impl SiteAccess {
    /// `all` (or missing) → everything; a list → just those sites.
    pub fn from_frontmatter(value: Option<&Value>) -> Self {
        match value {
            Some(Value::Array(items)) => Self::Only(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            Some(Value::String(s)) if s != "all" => Self::Only(vec![s.clone()]),
            _ => Self::All,
        }
    }

    pub fn can_read(&self, site: &str) -> bool {
        ALWAYS_READABLE.contains(&site) || self.can_write(site)
    }

    pub fn can_write(&self, site: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(sites) => sites.iter().any(|s| s == site),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_checks() {
        assert_eq!(SiteAccess::from_frontmatter(None), SiteAccess::All);
        assert_eq!(
            SiteAccess::from_frontmatter(Some(&json!("all"))),
            SiteAccess::All
        );
        let only = SiteAccess::from_frontmatter(Some(&json!(["memory", "plans"])));
        assert!(only.can_write("memory"));
        assert!(!only.can_write("home"));
        assert!(only.can_read("home"));
        assert!(!only.can_read("orchestration"));
    }
}
