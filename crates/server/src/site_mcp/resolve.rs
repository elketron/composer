//! An approved `mcp.toml` as `[mcp.*]`-shaped configs: whichever way a server is named, it ends
//! as the same config a `config.toml` entry makes, so there is one kind of server to connect.

use std::collections::BTreeMap;

use crate::McpServerConfig;
use crate::mcp_bundles::InstalledMcpBundles;
use crate::site_mcp::{ApprovedMcp, McpTarget, SiteServer, SiteServers};

/// The servers of `approved`. `global` is `config.toml`'s `[mcp.*]`: a site's server may not take
/// one of its names (their tools would clash), and a bundle's settings are its
/// `[mcp.<bundle>.config]`. `installed` is what is under `<home>/mcps/`; `user_home` is what a
/// bundle's `${HOME}` means.
pub fn resolve(
    approved: &ApprovedMcp,
    global: &BTreeMap<String, McpServerConfig>,
    installed: &InstalledMcpBundles,
    user_home: &str,
) -> SiteServers {
    let mut resolved = SiteServers {
        site: approved.site().to_string(),
        hash: approved.hash().to_string(),
        ..SiteServers::default()
    };
    for declared in approved.servers() {
        let name = &declared.name;
        if global.contains_key(name) {
            resolved.errors.push(format!(
                "MCP server {name} is left out: [mcp.{name}] in config.toml has that name, and \
                 its tools are in every session already"
            ));
            continue;
        }
        let config = match &declared.target {
            McpTarget::Bundle(bundle) => {
                let settings = global
                    .get(bundle)
                    .map(|entry| entry.config.clone())
                    .unwrap_or_default();
                match installed.server(bundle, &settings, user_home) {
                    Some(Ok(config)) => config,
                    Some(Err(e)) => {
                        resolved.errors.push(format!("MCP server {name}: {e}"));
                        continue;
                    }
                    None => {
                        resolved.errors.push(format!(
                            "MCP server {name}: no bundle `{bundle}` is installed (an .mcpb in \
                             the home's mcps/ folder)"
                        ));
                        continue;
                    }
                }
            }
            McpTarget::Stdio { command, args, env } => McpServerConfig {
                command: command.clone(),
                args: args.clone(),
                env: env.clone(),
                ..Default::default()
            },
            McpTarget::Http { url, headers } => McpServerConfig {
                url: url.clone(),
                headers: headers.clone(),
                ..Default::default()
            },
        };
        resolved.servers.push(SiteServer {
            name: name.clone(),
            config: McpServerConfig {
                allow: declared.allow.clone(),
                read_only: declared.read_only.clone(),
                enabled: true,
                ..config
            },
        });
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site_mcp::{SiteMcpServer, parse};
    use serde_json::json;
    use std::io::Write;
    use std::path::Path;

    fn approved(text: &str) -> ApprovedMcp {
        let parsed = parse(text.as_bytes(), Path::new("/sites/lab/mcp.toml"));
        assert_eq!(parsed.errors, Vec::<String>::new());
        ApprovedMcp::new("lab", "a-hash", parsed.servers)
    }

    fn installed(dir: &Path) -> InstalledMcpBundles {
        std::fs::create_dir_all(dir.join("mcps")).unwrap();
        InstalledMcpBundles::load(&dir.join("mcps"), &dir.join("state/mcps"))
    }

    #[test]
    fn each_kind_of_server_becomes_a_config() {
        let dir = tempfile::tempdir().unwrap();
        let made = resolve(
            &approved(
                "[servers.plain]\ncommand = \"npx\"\nargs = [\"-y\", \"pkg\"]\nallow = [\"get_*\"]\n\
                 read_only = [\"get_*\"]\n[servers.web]\nurl = \"http://127.0.0.1:5174/mcp\"\n",
            ),
            &BTreeMap::new(),
            &installed(dir.path()),
            "/home/me",
        );
        assert_eq!(made.errors, Vec::<String>::new());
        assert_eq!((made.site.as_str(), made.hash.as_str()), ("lab", "a-hash"));
        assert_eq!(
            made.servers[0],
            SiteServer {
                name: "plain".into(),
                config: McpServerConfig {
                    command: "npx".into(),
                    args: vec!["-y".into(), "pkg".into()],
                    allow: vec!["get_*".into()],
                    read_only: vec!["get_*".into()],
                    enabled: true,
                    ..Default::default()
                },
            }
        );
        assert_eq!(made.servers[1].config.url, "http://127.0.0.1:5174/mcp");
    }

    #[test]
    fn a_bundle_is_found_by_name_with_its_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mcps = dir.path().join("mcps");
        std::fs::create_dir_all(&mcps).unwrap();
        write_bundle(&mcps.join("greeter.mcpb"));
        let global: BTreeMap<String, McpServerConfig> = [(
            "greeter".to_string(),
            McpServerConfig {
                config: [("greeting".to_string(), "hi".to_string())].into(),
                enabled: false,
                ..Default::default()
            },
        )]
        .into();
        let approved = ApprovedMcp::new(
            "lab",
            "h",
            vec![SiteMcpServer {
                name: "greet".into(),
                target: McpTarget::Bundle("greeter".into()),
                allow: vec![],
                read_only: vec![],
            }],
        );
        let made = resolve(&approved, &global, &installed(dir.path()), "/home/me");
        assert_eq!(made.errors, Vec::<String>::new());
        let config = &made.servers[0].config;
        assert_eq!(config.command, "python3");
        assert_eq!(config.env["GREETING"], "hi");
        assert!(config.enabled && config.problem.is_none());

        // Not installed: said, and left out.
        let missing = approved_bundle("nope");
        let made = resolve(&missing, &global, &installed(dir.path()), "/home/me");
        assert!(made.servers.is_empty());
        assert!(
            made.errors[0].contains("no bundle `nope`"),
            "{:?}",
            made.errors
        );
    }

    #[test]
    fn a_name_config_toml_uses_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let global: BTreeMap<String, McpServerConfig> =
            [("wayai".to_string(), McpServerConfig::default())].into();
        let made = resolve(
            &approved("[servers.wayai]\ncommand = \"evil\"\n"),
            &global,
            &installed(dir.path()),
            "/h",
        );
        assert!(made.servers.is_empty());
        assert!(made.errors[0].contains("[mcp.wayai]"), "{:?}", made.errors);
    }

    fn approved_bundle(name: &str) -> ApprovedMcp {
        approved(&format!("[servers.b]\nbundle = \"{name}\"\n"))
    }

    fn write_bundle(path: &Path) {
        let manifest = json!({
            "name": "greeter",
            "server": { "type": "python", "mcp_config": {
                "command": "python3",
                "env": { "GREETING": "${user_config.greeting}" } } },
            "user_config": { "greeting": { "required": true } },
        });
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("server.py", options).unwrap();
        zip.write_all(b"print()").unwrap();
        zip.finish().unwrap();
    }
}
