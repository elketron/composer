use std::path::{Path, PathBuf};

use crate::workspace_template_file::TemplateFile;

use crate::home::Home;
use crate::library::{Library, SiteName, WorkspaceLibrary};
use crate::workspace_home::WorkspaceHome;

/// A recipe for a new workspace (`global/workspace-templates/<name>.toml`): the library sites it
/// starts with (on top of the stock ones every workspace gets) and its config overlay.
///
/// ```toml
/// description = "3D printing: CAD and assets"
/// sites = ["cad", "asset"]
///
/// [config.projects."~/Projects/printer"]
/// external_harnesses = []
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceTemplate {
    name: String,
    description: String,
    sites: Vec<SiteName>,
    config: toml::Table,
}

impl WorkspaceTemplate {
    /// Where templates live.
    pub fn folder(home: &Home) -> PathBuf {
        home.global().join("workspace-templates")
    }

    /// Template `name` of `home`.
    pub fn load(home: &Home, name: &str) -> Result<Self, String> {
        let plain = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !plain {
            return Err(format!("`{name}` is not a template name"));
        }
        let path = Self::folder(home).join(format!("{name}.toml"));
        let text = std::fs::read_to_string(&path).map_err(|_| {
            format!(
                "there is no workspace template `{name}` ({}): `composer workspace templates` \
                 lists them",
                path.display()
            )
        })?;
        Self::parse(name, &text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        let file: TemplateFile = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self {
            name: name.to_string(),
            description: file.description,
            sites: file.sites,
            config: file.config,
        })
    }

    /// Every template of `home`, by name; ones that don't parse are listed with the error as their
    /// description.
    pub fn all(home: &Home) -> Vec<(String, String)> {
        let mut names: Vec<String> = std::fs::read_dir(Self::folder(home))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                name.strip_suffix(".toml").map(str::to_string)
            })
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|name| match Self::load(home, &name) {
                Ok(template) => (name, template.description),
                Err(e) => (name, format!("(unreadable: {e})")),
            })
            .collect()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sites(&self) -> &[SiteName] {
        &self.sites
    }

    /// Fails, naming them, when the library lacks sites the template lists.
    pub fn check(&self, library: &Library) -> Result<(), String> {
        let have: Vec<String> = library.sites()?.into_iter().map(|(s, _)| s).collect();
        let missing: Vec<&str> = self
            .sites
            .iter()
            .map(SiteName::as_str)
            .filter(|s| !have.iter().any(|h| h == s))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        Err(format!(
            "workspace template `{}` lists site(s) the library doesn't have: {}. Publish them \
             from a workspace first, or take them out of the template",
            self.name,
            missing.join(", ")
        ))
    }

    /// Installs the template's sites into `workspace` (a new one: stock sites are already there)
    /// and writes its config overlay under the workspace's `config.toml`, checked by loading it.
    pub async fn apply(&self, workspace: &WorkspaceHome) -> Result<(), String> {
        let library = WorkspaceLibrary::of(workspace)?;
        self.check(library.library())?;
        let approvals = library.approvals()?;
        for site in &self.sites {
            if workspace.sites().join(site.as_str()).exists() {
                continue;
            }
            library.install(site, None, &approvals).await?;
        }
        if !self.config.is_empty() {
            append_config(&workspace.config_file(), &self.name, &self.config)?;
            workspace
                .load_config()
                .map_err(|e| format!("workspace template `{}`'s [config]: {e}", self.name))?;
        }
        Ok(())
    }
}

fn append_config(path: &Path, template: &str, config: &toml::Table) -> Result<(), String> {
    let mut text = std::fs::read_to_string(path).unwrap_or_default();
    let overlay = toml::to_string(config).map_err(|e| e.to_string())?;
    text.push_str(&format!(
        "\n# From workspace template `{template}`.\n{overlay}"
    ));
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_refuses_unknown_keys_and_bad_sites() {
        let template = WorkspaceTemplate::parse(
            "3d",
            "description = \"3D\"\nsites = [\"cad\"]\n[config.agent]\nslots = 2\n",
        )
        .unwrap();
        assert_eq!(template.sites(), [SiteName::new("cad").unwrap()]);
        assert!(WorkspaceTemplate::parse("x", "site = [\"cad\"]\n").is_err());
        assert!(WorkspaceTemplate::parse("x", "sites = [\"../cad\"]\n").is_err());
    }
}
