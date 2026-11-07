use serde::Deserialize;

use crate::library::SiteName;

/// A template's TOML file, as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateFile {
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) sites: Vec<SiteName>,
    #[serde(default)]
    pub(crate) config: toml::Table,
}
