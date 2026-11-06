use std::fmt;

use composer_core::SiteUrl;

/// A site's name, by the web's rules (letters, digits, `-`, `_`, `.`, not starting with `.`): the
/// folder of a site in the library or a workspace. Never `..` or a path, so `<sites>/<name>` stays
/// inside its folder.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct SiteName(String);

impl SiteName {
    pub fn new(raw: &str) -> Result<Self, String> {
        let name = raw
            .trim()
            .trim_start_matches("site://")
            .trim_end_matches('/');
        let url = SiteUrl::new(name, "").map_err(|e| e.to_string())?;
        if url.site() != name || name.contains('/') {
            return Err(format!("`{raw}` is not a site name"));
        }
        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SiteName {
    type Error = String;
    fn try_from(raw: String) -> Result<Self, String> {
        Self::new(&raw)
    }
}

impl From<SiteName> for String {
    fn from(name: SiteName) -> String {
        name.0
    }
}

impl fmt::Display for SiteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_site_names() {
        assert_eq!(SiteName::new("site://cad/").unwrap().as_str(), "cad");
        assert_eq!(SiteName::new("my-site_2").unwrap().as_str(), "my-site_2");
        for bad in ["", "..", ".git", "a/b", "../x", "a b", "/etc"] {
            assert!(SiteName::new(bad).is_err(), "{bad}");
        }
    }
}
