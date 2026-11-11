use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use composer_protocol::http::McpServerInfo;
use composer_protocol::{ApprovalDecision, ApprovalKind};
use serde_json::Value;

use crate::McpServerConfig;
use crate::agent::{AgentDefinition, Site};
use crate::mcp_clients::McpClient;
use crate::mcp_clients::types::{Slot, glob_matches, tool_name, transport_kind};
use crate::provider::ToolSpec;
use crate::sandbox::SandboxSpec;
use crate::tools::ToolContext;
use crate::tools::{CallResult, text_result};

/// After a failed connect, turns don't try again for this long (a call does).
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The MCP servers of `config.toml`. Each connects at its first use and stays up; one that
/// exited or failed connects again on the next use.
pub struct McpServers {
    configs: BTreeMap<String, McpServerConfig>,
    /// Shared with the next config's `McpServers` while the server's config is unchanged.
    slots: BTreeMap<String, Arc<tokio::sync::Mutex<Slot>>>,
    /// The workspace's environment ([`crate::WorkspaceHome::env`]): set for every stdio server it
    /// starts, and what `${COMPOSER_WORKSPACE}` in a config expands to.
    workspace: BTreeMap<String, String>,
    /// The world a stdio server starts in, as it is when it starts (other workspaces' projects
    /// change).
    sandbox: SandboxWorld,
}

/// Makes the [`SandboxSpec`] of a spawn now.
pub type SandboxWorld = Arc<dyn Fn() -> SandboxSpec + Send + Sync>;

impl McpServers {
    pub fn new(
        configs: &BTreeMap<String, McpServerConfig>,
        workspace: BTreeMap<String, String>,
        sandbox: SandboxWorld,
    ) -> Self {
        let mut usable = BTreeMap::new();
        for (name, config) in configs {
            let valid = !name.is_empty()
                && !name.contains("__")
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
            if valid {
                usable.insert(name.clone(), config.clone());
            } else {
                tracing::warn!("[mcp.{name}]: a server name is letters, digits, - and _ (no `__`)");
            }
        }
        Self {
            slots: usable
                .iter()
                .filter(|(_, c)| c.enabled)
                .map(|(name, _)| (name.clone(), Arc::default()))
                .collect(),
            configs: usable,
            workspace,
            sandbox,
        }
    }

    /// The servers of a new config: an unchanged server keeps its connection, a changed or
    /// removed one is dropped (its process stops once no call is using it).
    pub fn reconfigured(&self, configs: &BTreeMap<String, McpServerConfig>) -> Self {
        let mut next = Self::new(configs, self.workspace.clone(), self.sandbox.clone());
        for (name, slot) in &mut next.slots {
            if let Some(kept) = self.slots.get(name)
                && self.configs.get(name) == next.configs.get(name)
            {
                *slot = kept.clone();
            }
        }
        next
    }

    /// The server's connection, made now if there is none. `retry`: also when it failed a
    /// moment ago.
    async fn client(&self, name: &str, retry: bool) -> Result<Arc<McpClient>, String> {
        let slot = self
            .slots
            .get(name)
            .ok_or_else(|| format!("no MCP server `{name}` is on"))?;
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
        match McpClient::connect(
            name,
            &self.configs[name],
            &self.workspace,
            &(self.sandbox)(),
        )
        .await
        {
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

    /// Whether `[mcp.<server>]` lets the site called `site` grant its tools (`sites` lists it).
    /// The page's `mcp:` alone grants nothing: agents can write pages, not `config.toml`.
    pub fn site_may_grant(&self, site: &str, server: &str) -> bool {
        self.configs
            .get(server)
            .is_some_and(|config| config.sites.iter().any(|allowed| allowed == site))
    }

    /// The servers `site` names in `mcp:` and may grant.
    pub fn granted_by(&self, site: &Site) -> Vec<String> {
        site.mcp
            .iter()
            .filter(|server| self.site_may_grant(&site.name, server))
            .cloned()
            .collect()
    }

    /// For each server `site` names in `mcp:` and does not get: why, in a sentence for the user.
    pub fn refused_grants(&self, site: &Site) -> Vec<String> {
        site.mcp
            .iter()
            .filter(|server| !self.site_may_grant(&site.name, server))
            .map(|server| match self.configs.contains_key(server) {
                true => format!(
                    "site {0} names MCP server {server}, but [mcp.{server}] does not list it in \
                     `sites`: its tools are not added (add `sites = [\"{0}\"]` there to allow it)",
                    site.name
                ),
                false => format!(
                    "site {} names MCP server {server}, but there is no [mcp.{server}] in \
                     config.toml: its tools are not added",
                    site.name
                ),
            })
            .collect()
    }

    /// Whether `agent`, in `site` (the one its session is in), has the MCP tool `tool`
    /// (`blender__get_objects`): its page's `tools:` lists it, or the site names the tool's
    /// server in `mcp:` and the server's config lets that site grant it. Every place that decides
    /// MCP tool availability asks this one.
    pub fn allows(&self, agent: &AgentDefinition, site: Option<&Site>, tool: &str) -> bool {
        agent.allows_mcp_tool(tool)
            || site.is_some_and(|site| {
                tool.split_once("__").is_some_and(|(server, _)| {
                    site.mcp.iter().any(|named| named == server)
                        && self.site_may_grant(&site.name, server)
                })
            })
    }

    /// The MCP tools `agent` lists, and every tool of the servers `site` (the one the session is
    /// in) names in `mcp:` and may grant ([`Self::granted_by`]), from servers that are
    /// configured, on and reachable now.
    pub async fn specs_for(&self, agent: &AgentDefinition, site: Option<&Site>) -> Vec<ToolSpec> {
        let mut servers = agent.mcp_servers();
        if let Some(site) = site {
            servers.extend(self.granted_by(site));
        }
        servers.sort();
        servers.dedup();
        let mut specs = Vec::new();
        for server in servers {
            if !self.slots.contains_key(&server) {
                continue;
            }
            // A failed connect is logged where it happens; the turn goes on without it.
            let Ok(client) = self.client(&server, false).await else {
                continue;
            };
            for tool in client.tools().await {
                let name = tool_name(&server, &tool.name);
                if self.allows(agent, site, &name) {
                    specs.push(ToolSpec {
                        name,
                        description: format!("[{server}] {}", tool.description),
                        parameters: tool.input_schema,
                    });
                }
            }
        }
        specs
    }

    /// Runs `name` (`github__get_issue`) if it is an MCP tool: `None` when it isn't one.
    pub async fn call(&self, context: &ToolContext, name: &str, args: Value) -> Option<CallResult> {
        let (server, _) = name.split_once("__")?;
        if !self.configs.contains_key(server) {
            return None;
        }
        if !self.allows(&context.agent, context.site.as_deref(), name) {
            return Some(text_result(format!("unknown tool `{name}`"), true));
        }
        Some(
            self.call_tool(context, server, name, args)
                .await
                .unwrap_or_else(|e| text_result(e, true)),
        )
    }

    /// A call the server makes itself (an `app:` launch): by the tool's own name (`launch`), no
    /// agent `tools:` check and no approval. The caller's authority is a `config.toml` entry.
    /// Never reachable from a model's tool call: [`Self::call`] is that path.
    pub async fn call_internal(
        &self,
        server: &str,
        tool: &str,
        args: Value,
    ) -> Result<CallResult, String> {
        if !self.configs.contains_key(server) {
            return Err(format!("no MCP server `{server}` in config.toml"));
        }
        let client = self.client(server, true).await?;
        let found = client.tools().await.into_iter().any(|t| t.name == tool);
        if !found {
            return Err(format!("MCP server `{server}` has no tool `{tool}`"));
        }
        client.call(tool, args).await
    }

    async fn call_tool(
        &self,
        context: &ToolContext,
        server: &str,
        name: &str,
        args: Value,
    ) -> Result<CallResult, String> {
        let client = self.client(server, true).await?;
        let tool = client
            .tools()
            .await
            .into_iter()
            .find(|t| tool_name(server, &t.name) == name)
            .ok_or_else(|| format!("`{server}` has no tool `{name}` (any more)"))?;
        self.approve(context, server, &tool.name, name, &args)
            .await?;
        client.call(&tool.name, args).await
    }

    /// Asks the user unless `allow` or an earlier `always` covers the tool.
    async fn approve(
        &self,
        context: &ToolContext,
        server: &str,
        tool: &str,
        name: &str,
        args: &Value,
    ) -> Result<(), String> {
        let server_ref = &context.server;
        let rules_key = format!("mcp:{server}");
        let allowed = self.configs[server]
            .allow
            .iter()
            .any(|pattern| glob_matches(pattern, tool));
        if allowed || server_ref.tool_rules.allows(&rules_key, name, args) {
            return Ok(());
        }
        let body = serde_json::to_string_pretty(args).unwrap_or_default();
        let decision = server_ref
            .ask_approval(
                &context.session,
                ApprovalKind::Mcp,
                format!("MCP {server}: {tool}"),
                body,
                Some(name.to_string()),
                &context.cancel,
            )
            .await;
        match decision.map(|answer| (answer.decision, answer.reason)) {
            Some((ApprovalDecision::Approve, _)) => Ok(()),
            Some((ApprovalDecision::Always, _)) => {
                if let Err(e) = server_ref.tool_rules.add(&rules_key, name) {
                    tracing::warn!("saving the rule for {name}: {e}");
                }
                Ok(())
            }
            Some((ApprovalDecision::Deny, Some(reason))) => Err(format!(
                "Your orchestrator denied `{name}`: {reason}. Don't try to get around it: do \
                 without, or say what you needed it for."
            )),
            Some((ApprovalDecision::Deny, None)) => Err(format!(
                "The user denied `{name}`. Don't try to get around it: do without, or say what \
                 you needed it for."
            )),
            None => Err("cancelled".into()),
        }
    }

    /// Every configured server with its state, connecting the ones that are on.
    pub async fn list(&self, agents: &[AgentDefinition]) -> Vec<McpServerInfo> {
        let mut infos = Vec::new();
        for (name, config) in &self.configs {
            let mut info = McpServerInfo {
                name: name.clone(),
                transport: transport_kind(&config.command).into(),
                state: "off".into(),
                error: None,
                tools: vec![],
                agents: agents
                    .iter()
                    .filter(|a| a.mcp_servers().contains(name))
                    .map(|a| a.name.clone())
                    .collect(),
            };
            if config.enabled {
                match self.client(name, true).await {
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
