use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use composer_protocol::http::McpServerInfo;
use composer_protocol::{ApprovalDecision, ApprovalKind};
use serde_json::Value;

use crate::McpServerConfig;
use crate::agent::AgentDefinition;
use crate::mcp_clients::McpClient;
use crate::mcp_clients::types::{Slot, glob_matches, tool_name, transport_kind};
use crate::provider::ToolSpec;
use crate::tools::ToolContext;

/// After a failed connect, turns don't try again for this long (a call does).
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The MCP servers of `config.toml`. Each connects at its first use and stays up; one that
/// exited or failed connects again on the next use.
pub struct McpServers {
    configs: BTreeMap<String, McpServerConfig>,
    /// Shared with the next config's `McpServers` while the server's config is unchanged.
    slots: BTreeMap<String, Arc<tokio::sync::Mutex<Slot>>>,
}

impl McpServers {
    pub fn new(configs: &BTreeMap<String, McpServerConfig>) -> Self {
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
        }
    }

    /// The servers of a new config: an unchanged server keeps its connection, a changed or
    /// removed one is dropped (its process stops once no call is using it).
    pub fn reconfigured(&self, configs: &BTreeMap<String, McpServerConfig>) -> Self {
        let mut next = Self::new(configs);
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
        match McpClient::connect(name, &self.configs[name]).await {
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

    /// The MCP tools `agent` lists, from servers it can reach now.
    pub async fn specs_for(&self, agent: &AgentDefinition) -> Vec<ToolSpec> {
        let mut specs = Vec::new();
        for server in agent.mcp_servers() {
            if !self.slots.contains_key(&server) {
                continue;
            }
            // A failed connect is logged where it happens; the turn goes on without it.
            let Ok(client) = self.client(&server, false).await else {
                continue;
            };
            for tool in client.tools().await {
                let name = tool_name(&server, &tool.name);
                if agent.allows_mcp_tool(&name) {
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
    pub async fn call(
        &self,
        context: &ToolContext,
        name: &str,
        args: Value,
    ) -> Option<(String, bool)> {
        let (server, _) = name.split_once("__")?;
        if !self.configs.contains_key(server) {
            return None;
        }
        if !context.agent.allows_mcp_tool(name) {
            return Some((format!("unknown tool `{name}`"), true));
        }
        Some(
            self.call_tool(context, server, name, args)
                .await
                .unwrap_or_else(|e| (e, true)),
        )
    }

    async fn call_tool(
        &self,
        context: &ToolContext,
        server: &str,
        name: &str,
        args: Value,
    ) -> Result<(String, bool), String> {
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
