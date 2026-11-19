//! MCP Bundles (`.mcpb`): zip archives with a `manifest.json` that says how to start a local MCP
//! server. `<home>/mcps/*.mcpb` are unpacked to `<home>/state/mcps/<name>/` and merged into
//! the `[mcp.<name>]` entries of every workspace's config ([`InstalledMcpBundles::merge_into`]).
//!
//! A bundle's server writes to `<home>/state/mcps/.run/<name>/` ([`McpBundle::config`]), not
//! next to its unpacked code.
//! Archives are unpacked within [`BUNDLE_LIMITS`].

mod installed_mcp_bundles;
mod mcp_bundle;
mod mcp_bundle_file;
mod mcp_bundle_manifest;
mod slug;
mod substitute;

pub use installed_mcp_bundles::InstalledMcpBundles;
pub use mcp_bundle::McpBundle;
pub use mcp_bundle_file::{BUNDLE_LIMITS, BundleLimits, McpBundleFile};
pub use mcp_bundle_manifest::{McpBundleManifest, UserSetting};
pub use slug::slug;
