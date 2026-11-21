use std::sync::Arc;

use composer_protocol::ToolInfo;
use serde_json::Value;

use crate::agent::{AgentDefinition, Site};
use crate::mcp_clients::McpServers;
use crate::provider::{ModelToolCall, ToolSpec};
use crate::server::Server;
use crate::tools::actions_tool::ActionsTool;
use crate::tools::arg_repair::RepairedArgs;
use crate::tools::desk_tool::DeskTool;
use crate::tools::edit_tool::EditTool;
use crate::tools::enter_site_tool::EnterSiteTool;
use crate::tools::leave_site_tool::LeaveSiteTool;
use crate::tools::list_sites_tool::ListSitesTool;
use crate::tools::new_site_tool::NewSiteTool;
use crate::tools::open_tool::OpenTool;
use crate::tools::run_tool::RunTool;
use crate::tools::search_tool::SearchTool;
use crate::tools::show_page_tool::ShowPageTool;
use crate::tools::terminal_read_tool::TerminalReadTool;
use crate::tools::terminal_send_tool::TerminalSendTool;
use crate::tools::watch_tool::WatchTool;
use crate::tools::write_tool::WriteTool;
use crate::tools::{CallResult, Tool, ToolContext, text_result};
use crate::watches::WRITER;

/// All tools the server provides. An agent gets the ones its `tools:` lists.
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn standard() -> Self {
        Self {
            tools: vec![
                Arc::new(OpenTool),
                Arc::new(SearchTool),
                Arc::new(ListSitesTool),
                Arc::new(WriteTool),
                Arc::new(EditTool),
                Arc::new(EnterSiteTool),
                Arc::new(LeaveSiteTool),
                Arc::new(ActionsTool),
                Arc::new(RunTool),
                Arc::new(NewSiteTool),
                Arc::new(WatchTool),
                Arc::new(ShowPageTool),
                Arc::new(DeskTool),
                Arc::new(TerminalReadTool),
                Arc::new(TerminalSendTool),
            ],
        }
    }

    pub fn specs_for(&self, agent: &AgentDefinition, site: Option<&Site>) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|t| Self::allowed(agent, site, t.name()))
            .map(|t| ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect()
    }

    /// The agent's base `tools:` plus the tools of the site it entered.
    fn allowed(agent: &AgentDefinition, site: Option<&Site>, tool: &str) -> bool {
        agent.allows_tool(tool) || site.is_some_and(|site| site.allows_tool(tool))
    }

    /// The native tools `agent` lists, then the MCP tools it lists (servers it can reach).
    pub async fn specs_with_mcp(
        &self,
        server: &Server,
        agent: &AgentDefinition,
        site: Option<&Site>,
    ) -> Vec<ToolSpec> {
        let mut specs = self.specs_for(agent, site);
        specs.extend(server.mcp().specs_for(agent, site).await);
        specs
    }

    /// The same, for the protocol (`tool.list`).
    pub async fn infos_for(
        &self,
        mcp: &McpServers,
        agent: &AgentDefinition,
        site: Option<&Site>,
    ) -> Vec<ToolInfo> {
        let mut specs = self.specs_for(agent, site);
        specs.extend(mcp.specs_for(agent, site).await);
        specs
            .into_iter()
            .map(|spec| ToolInfo {
                name: spec.name,
                description: spec.description,
                parameters: spec.parameters,
            })
            .collect()
    }

    /// Runs a model's call. Returns the result text and whether it failed.
    pub async fn call(&self, context: &ToolContext, call: &ModelToolCall) -> CallResult {
        let args = match serde_json::from_str(&call.arguments) {
            Ok(args) => args,
            Err(e) => return text_result(format!("arguments are not valid JSON: {e}"), true),
        };
        // Our own tools' schemas say what a garbled call meant; MCP tools get it as it came.
        let Some(schema) = self
            .tools
            .iter()
            .find(|t| t.name() == call.name)
            .map(|t| t.parameters())
        else {
            return self.call_value(context, &call.name, args).await;
        };
        let repaired = RepairedArgs::new(args, &schema);
        let mut result = self
            .call_value(context, &call.name, repaired.args.clone())
            .await;
        if result.is_error
            && let Some(note) = repaired.unknown_note(&call.name, &schema)
        {
            result.text.push_str(&format!("\n{note}"));
        }
        result
    }

    /// Runs a call with parsed arguments (models, and harnesses through `tool.call`).
    pub async fn call_value(&self, context: &ToolContext, name: &str, args: Value) -> CallResult {
        let Some(tool) = self.tools.iter().find(|t| {
            t.name() == name && Self::allowed(&context.agent, context.site.as_deref(), t.name())
        }) else {
            if let Some(result) = context.server.mcp().call(context, name, args).await {
                return result;
            }
            return text_result(format!("unknown tool `{name}`"), true);
        };
        // Its own writes don't wake its watches.
        WRITER
            .scope(
                context.session.id().to_string(),
                tool.call_result(context, args),
            )
            .await
    }
}
