use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use composer_protocol::http::McpServerInfo;
use composer_protocol::{ApprovalDecision, ApprovalKind, RunKind};
use serde_json::Value;

use crate::McpServerConfig;
use crate::mcp_clients::McpClient;
use crate::mcp_clients::types::{Slot, glob_matches, tool_name, transport_kind};
use crate::provider::ToolSpec;
use crate::site_mcp::SiteServers;
use crate::tools::ToolContext;
use crate::tools::{CallResult, text_result};

/// After a failed connect, turns don't try again for this long (a call does).
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// Connections, one per distinct server config, shared by every server entry that starts the
/// same thing (two sites naming the same command get one process).
type Pool = Mutex<HashMap<String, Arc<tokio::sync::Mutex<Slot>>>>;

/// One server a run can reach: from `config.toml` (`site` is `None`) or from the `mcp.toml` of
/// the site the run is in.
struct Entry<'a> {
    name: &'a str,
    config: &'a McpServerConfig,
    site: Option<&'a str>,
}

/// The MCP servers: `config.toml`'s, in every session, and the approved servers of a run's site
/// ([`crate::site_mcp`]). Each connects at its first use and stays up in a pool keyed by what it
/// starts; one that exited or failed connects again on the next use.
pub struct McpServers {
    configs: BTreeMap<String, McpServerConfig>,
    /// Shared with the next config's `McpServers`.
    pool: Arc<Pool>,
    /// The home's environment ([`crate::Home::env`]): set for every stdio server it starts, and
    /// what `${COMPOSER_HOME}` in a config expands to.
    workspace: BTreeMap<String, String>,
    /// The sites folder: where a stdio server without a `cwd` starts.
    sites: std::path::PathBuf,
}

impl McpServers {
    pub fn new(
        configs: &BTreeMap<String, McpServerConfig>,
        workspace: BTreeMap<String, String>,
        sites: std::path::PathBuf,
    ) -> Self {
        let mut usable = BTreeMap::new();
        for (name, config) in configs {
            if crate::site_mcp::valid_name(name) {
                usable.insert(name.clone(), config.clone());
            } else {
                tracing::warn!("[mcp.{name}]: a server name is letters, digits, - and _ (no `__`)");
            }
        }
        Self {
            configs: usable,
            pool: Arc::default(),
            workspace,
            sites,
        }
    }

    /// The servers of a new config: a connection whose config is still a `config.toml` server's
    /// is kept, every other one is dropped (its process stops once no call is using it). Sites'
    /// servers are made again from the new config on next use.
    pub fn reconfigured(&self, configs: &BTreeMap<String, McpServerConfig>) -> Self {
        let next = Self::new(configs, self.workspace.clone(), self.sites.clone());
        let keep: Vec<String> = next.configs.values().map(pool_key).collect();
        let old = self.pool.lock().expect("pool lock");
        *next.pool.lock().expect("pool lock") = old
            .iter()
            .filter(|(key, _)| keep.contains(key))
            .map(|(key, slot)| (key.clone(), slot.clone()))
            .collect();
        drop(old);
        next
    }

    /// `config.toml`'s servers that are on, then `site`'s (a name `config.toml` uses is left out
    /// of a site's when it is resolved).
    fn entries<'a>(&'a self, site: Option<&'a SiteServers>) -> Vec<Entry<'a>> {
        let global = self
            .configs
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, config)| Entry {
                name,
                config,
                site: None,
            });
        let own = site.into_iter().flat_map(|site| {
            site.servers.iter().map(|server| Entry {
                name: &server.name,
                config: &server.config,
                site: Some(&site.site),
            })
        });
        global.chain(own).collect()
    }

    /// The connection of `config`, made now if there is none. `retry`: also when it failed a
    /// moment ago.
    async fn client(
        &self,
        name: &str,
        config: &McpServerConfig,
        retry: bool,
    ) -> Result<Arc<McpClient>, String> {
        let slot = self
            .pool
            .lock()
            .expect("pool lock")
            .entry(pool_key(config))
            .or_default()
            .clone();
        let mut slot = slot.lock().await;
        if let Some(client) = &slot.client
            && client.alive()
        {
            return Ok(client.clone());
        }
        if !retry
            && let (Some(error), Some(at)) = (&slot.error, slot.failed_at)
            && at.elapsed() < RETRY_AFTER
        {
            return Err(error.clone());
        }
        match McpClient::connect(name, config, &self.workspace, &self.sites).await {
            Ok(client) => {
                let client = Arc::new(client);
                tracing::info!(
                    "mcp {name}: connected, {} tools",
                    client.tools().await.len()
                );
                *slot = Slot {
                    client: Some(client.clone()),
                    ..Slot::default()
                };
                Ok(client)
            }
            Err(e) => {
                tracing::warn!("mcp {name}: {e}");
                *slot = Slot {
                    client: None,
                    error: Some(e.clone()),
                    failed_at: Some(Instant::now()),
                };
                Err(e)
            }
        }
    }

    /// Whether a run of `kind` has `tool` (as its server lists it) of a server configured as
    /// `config`: an agentic run every tool, a workflow run those `read_only` matches, a single
    /// shot none. Every place that decides MCP tool availability asks this one.
    pub fn kind_allows(kind: RunKind, config: &McpServerConfig, tool: &str) -> bool {
        match kind {
            RunKind::Agentic => true,
            RunKind::Workflow => config
                .read_only
                .iter()
                .any(|pattern| glob_matches(pattern, tool)),
            RunKind::SingleShot => false,
        }
    }

    /// Every tool a run of `kind` has from `config.toml`'s servers and `site`'s (the approved
    /// servers of the site it is in), from the servers reachable now.
    pub async fn specs_for(&self, kind: RunKind, site: Option<&SiteServers>) -> Vec<ToolSpec> {
        let mut specs = Vec::new();
        for entry in self.entries(site) {
            // Nothing to offer: don't start the server for it.
            if kind == RunKind::SingleShot
                || (kind == RunKind::Workflow && entry.config.read_only.is_empty())
            {
                continue;
            }
            // A failed connect is logged where it happens; the turn goes on without it.
            let Ok(client) = self.client(entry.name, entry.config, false).await else {
                continue;
            };
            for tool in client.tools().await {
                if Self::kind_allows(kind, entry.config, &tool.name) {
                    specs.push(ToolSpec {
                        name: tool_name(entry.name, &tool.name),
                        description: format!("[{}] {}", entry.name, tool.description),
                        parameters: tool.input_schema,
                    });
                }
            }
        }
        specs
    }

    /// Starts `site`'s servers (those not running yet), side by side. What did not start, each
    /// a sentence.
    pub async fn connect(&self, site: &SiteServers) -> Vec<String> {
        let tries = site.servers.iter().map(|server| async move {
            self.client(&server.name, &server.config, true)
                .await
                .err()
                .map(|e| format!("MCP server {} did not start: {e}", server.name))
        });
        futures::future::join_all(tries)
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    /// Runs `name` (`github__get_issue`) if it is an MCP tool of the run's servers (`site`: those
    /// of the site it is in): `None` when it isn't one.
    pub async fn call(
        &self,
        context: &ToolContext,
        site: Option<&SiteServers>,
        name: &str,
        args: Value,
    ) -> Option<CallResult> {
        let (server, _) = name.split_once("__")?;
        let entries = self.entries(site);
        let entry = entries.iter().find(|entry| entry.name == server)?;
        Some(
            self.call_tool(context, entry, name, args)
                .await
                .unwrap_or_else(|e| text_result(e, true)),
        )
    }

    async fn call_tool(
        &self,
        context: &ToolContext,
        entry: &Entry<'_>,
        name: &str,
        args: Value,
    ) -> Result<CallResult, String> {
        let client = self.client(entry.name, entry.config, true).await?;
        let tool = client
            .tools()
            .await
            .into_iter()
            .find(|t| tool_name(entry.name, &t.name) == name)
            .filter(|t| Self::kind_allows(context.kind, entry.config, &t.name))
            .ok_or_else(|| format!("unknown tool `{name}`"))?;
        self.approve(context, entry, &tool.name, name, &args)
            .await?;
        client.call(&tool.name, args).await
    }

    /// Asks the user unless `allow` or an earlier `always` covers the tool. A site's server keeps
    /// its `always` rules apart from `config.toml`'s.
    async fn approve(
        &self,
        context: &ToolContext,
        entry: &Entry<'_>,
        tool: &str,
        name: &str,
        args: &Value,
    ) -> Result<(), String> {
        let server_ref = &context.server;
        let rules_key = match entry.site {
            None => format!("mcp:{}", entry.name),
            Some(site) => format!("mcp:{site}/{}", entry.name),
        };
        let allowed = entry
            .config
            .allow
            .iter()
            .any(|pattern| glob_matches(pattern, tool));
        if allowed || server_ref.tool_rules.allows(&rules_key, name, args) {
            return Ok(());
        }
        let body = serde_json::to_string_pretty(args).unwrap_or_default();
        let decision = server_ref
            .ask_approval(
                &*context.approval_session(&format!("`{name}` (MCP {})", entry.name))?,
                ApprovalKind::Mcp,
                format!("MCP {}: {tool}", entry.name),
                body,
                Some(name.to_string()),
                &context.cancel,
            )
            .await;
        match decision {
            Some(ApprovalDecision::Approve) => Ok(()),
            Some(ApprovalDecision::Always) => {
                if let Err(e) = server_ref.tool_rules.add(&rules_key, name) {
                    tracing::warn!("saving the rule for {name}: {e}");
                }
                Ok(())
            }
            Some(ApprovalDecision::Deny) => Err(format!(
                "The user denied `{name}`. Don't try to get around it: do without, or say what \
                 you needed it for."
            )),
            None => Err("cancelled".into()),
        }
    }

    /// Every server with its state: `config.toml`'s (connecting the ones that are on), then those
    /// of the sites in `sites`.
    pub async fn list(&self, sites: &[Arc<SiteServers>]) -> Vec<McpServerInfo> {
        let mut infos = Vec::new();
        let mut all: Vec<(&String, &McpServerConfig, Option<String>)> = self
            .configs
            .iter()
            .map(|(name, config)| (name, config, None))
            .collect();
        for site in sites {
            for server in &site.servers {
                all.push((&server.name, &server.config, Some(site.site.clone())));
            }
        }
        for (name, config, site) in all {
            let mut info = McpServerInfo {
                name: name.clone(),
                transport: transport_kind(&config.command).into(),
                state: "off".into(),
                error: None,
                tools: vec![],
                sites: site.into_iter().collect(),
            };
            if config.enabled {
                match self.client(name, config, true).await {
                    Ok(client) => {
                        info.state = "ready".into();
                        info.tools = client
                            .tools()
                            .await
                            .iter()
                            .map(|t| tool_name(name, &t.name))
                            .collect();
                    }
                    Err(e) => {
                        info.state = "failed".into();
                        info.error = Some(e);
                    }
                }
            }
            infos.push(info);
        }
        infos
    }
}

/// What tells two server configs apart as processes: what they start or reach, not which of
/// their tools run unasked.
fn pool_key(config: &McpServerConfig) -> String {
    let started = McpServerConfig {
        allow: Vec::new(),
        read_only: Vec::new(),
        enabled: true,
        config: BTreeMap::new(),
        ..config.clone()
    };
    format!(
        "{}{}",
        serde_json::to_string(&started).expect("a config serializes"),
        started.problem.as_deref().unwrap_or_default()
    )
}
