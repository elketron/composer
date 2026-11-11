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

    /// Registers the bundles in `configs`, or overlays them on the entries of their name.
    pub fn merge_into(&self, configs: &mut BTreeMap<String, McpServerConfig>) {
        let user_home = std::env::var("HOME").unwrap_or_default();
        for bundle in &self.bundles {
            let run = self.run.join(&bundle.name);
            match bundle.merged(configs.get(&bundle.name), &user_home, &run) {
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
}
