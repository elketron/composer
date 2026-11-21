use std::collections::BTreeMap;
use std::path::Path;

use crate::McpServerConfig;
use crate::mcp_bundles::{McpBundle, McpBundleFile};

/// The bundles installed in a composer home.
pub struct InstalledMcpBundles {
    bundles: Vec<McpBundle>,
    /// Where each bundle's server may write (`state/mcps/.run/<name>/`): never its code.
    run: std::path::PathBuf,
}

impl InstalledMcpBundles {
    /// Unpacks (when needed) and reads every `*.mcpb` in `mcps`, into `state` (`state/mcps`).
    /// One that cannot be read is skipped with a warning.
    pub fn load(mcps: &Path, state: &Path) -> Self {
        let mut files: Vec<_> = std::fs::read_dir(mcps)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "mcpb"))
            .collect();
        files.sort();
        let mut bundles: Vec<McpBundle> = Vec::new();
        for file in files {
            match McpBundleFile::new(&file).load(state) {
                Ok(bundle) if bundles.iter().any(|b| b.name == bundle.name) => tracing::warn!(
                    "mcp bundle {}: another bundle is already named `{}`, skipped",
                    file.display(),
                    bundle.name
                ),
                Ok(bundle) => bundles.push(bundle),
                Err(e) => tracing::warn!("mcp bundle {}: skipped: {e}", file.display()),
            }
        }
        Self {
            bundles,
            // No bundle is named like it: a slug has no dot.
            run: state.join(".run"),
        }
    }

    /// Overlays the bundles on the `[mcp.<name>]` entries of their name, which makes them servers
    /// in every session. A bundle `config.toml` does not name is only a site's to use
    /// (`bundle = "<name>"` in its `mcp.toml`).
    pub fn merge_into(&self, configs: &mut BTreeMap<String, McpServerConfig>) {
        let user_home = std::env::var("HOME").unwrap_or_default();
        for bundle in &self.bundles {
            let Some(entry) = configs.get(&bundle.name) else {
                continue;
            };
            let run = self.run.join(&bundle.name);
            match bundle.merged(Some(entry), &user_home, &run) {
                Some(config) => {
                    // Made here, not at connect, so the bind has something to show.
                    if let Err(e) = bundle.prepare(&run) {
                        tracing::warn!("mcp bundle `{}`: {e}", bundle.name);
                    }
                    configs.insert(bundle.name.clone(), config);
                }
                None => tracing::warn!(
                    "[mcp.{0}] has its own command or url: the bundle of the same name is ignored",
                    bundle.name
                ),
            }
        }
    }

    /// The bundle called `name`, as a server: `settings` are its `[mcp.<name>.config]`. Its run
    /// folder is made here, so the server has one to write in. `None` when none is installed.
    pub fn server(
        &self,
        name: &str,
        settings: &BTreeMap<String, String>,
        user_home: &str,
    ) -> Option<Result<McpServerConfig, String>> {
        let bundle = self.bundles.iter().find(|b| b.name == name)?;
        let run = self.run.join(&bundle.name);
        Some(
            bundle
                .prepare(&run)
                .map(|()| bundle.config(settings, user_home, &run))
                .map_err(|e| format!("preparing the bundle `{name}`: {e}")),
        )
    }
}
