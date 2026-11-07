use std::fmt;

/// A workspace template's name: letters, digits, `-` and `_`, so
/// `global/workspace-templates/<name>.toml` stays inside its folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TemplateName(String);

impl TemplateName {
    pub fn new(raw: &str) -> Result<Self, String> {
        let plain = !raw.is_empty()
            && raw
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !plain {
            return Err(format!("`{raw}` is not a template name"));
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TemplateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_template_names() {
        assert_eq!(
            TemplateName::new("3d-print_2").unwrap().as_str(),
            "3d-print_2"
        );
        for bad in ["", "..", "a/b", "../x", "a b", "x.toml"] {
            assert!(TemplateName::new(bad).is_err(), "{bad}");
        }
    }
}
