use std::sync::Arc;

use composer_protocol::ToolInfo;
use serde_json::Value;

use crate::agent::AgentDefinition;
use crate::mcp_clients::McpServers;
use crate::server::Server;
use crate::provider::{ModelToolCall, ToolSpec};
use crate::tools::actions_tool::ActionsTool;
use crate::tools::edit_tool::EditTool;
use crate::tools::finish_tool::FinishTool;
use crate::tools::list_sites_tool::ListSitesTool;
use crate::tools::merge_ticket_tool::MergeTicketTool;
use crate::tools::new_app_tool::NewAppTool;
use crate::tools::new_pipeline_tool::NewPipelineTool;
use crate::tools::new_project_tool::NewProjectTool;
use crate::tools::new_site_tool::NewSiteTool;
use crate::tools::new_work_tool::NewWorkTool;
use crate::tools::open_tool::OpenTool;
use crate::tools::run_stage_tool::RunStageTool;
use crate::tools::run_tool::RunTool;
use crate::tools::search_tool::SearchTool;
use crate::tools::set_project_tool::SetProjectTool;
use crate::tools::spawn_worker_tool::SpawnWorkerTool;
use crate::tools::write_tool::WriteTool;
use crate::tools::{Tool, ToolContext};

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
                Arc::new(SetProjectTool),
                Arc::new(ActionsTool),
                Arc::new(RunTool),
                Arc::new(NewSiteTool),
                Arc::new(NewAppTool),
                Arc::new(NewProjectTool),
                Arc::new(NewPipelineTool),
                Arc::new(NewWorkTool),
                Arc::new(RunStageTool),
                Arc::new(SpawnWorkerTool),
                Arc::new(MergeTicketTool),
                Arc::new(FinishTool),
            ],
        }
    }

    pub fn specs_for(&self, agent: &AgentDefinition) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|t| agent.allows_tool(t.name()))
            .map(|t| ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect()
    }

    /// The native tools `agent` lists, then the MCP tools it lists (servers it can reach).
    /// A worker process gets its MCP tools from the parent, which runs them: one process per
    /// MCP server, and approvals where the clients are.
    pub async fn specs_with_mcp(
        &self,
        server: &Server,
        session: &str,
        agent: &AgentDefinition,
    ) -> Vec<ToolSpec> {
        let mut specs = self.specs_for(agent);
        match &server.parent {
            Some(parent) if !agent.mcp_servers().is_empty() => match parent.tools(session).await {
                Ok(tools) => specs.extend(
                    tools
                        .into_iter()
                        .filter(|t| t.name.contains("__"))
                        .map(|t| ToolSpec {
                            name: t.name,
                            description: t.description,
                            parameters: t.parameters,
                        }),
                ),
                Err(e) => tracing::warn!("MCP tools from the parent: {e}"),
            },
            Some(_) => {}
            None => specs.extend(server.mcp().specs_for(agent).await),
        }
        specs
    }

    /// The same, for the protocol (`tool.list`, answered by the parent).
    pub async fn infos_for(&self, mcp: &McpServers, agent: &AgentDefinition) -> Vec<ToolInfo> {
        let mut specs = self.specs_for(agent);
        specs.extend(mcp.specs_for(agent).await);
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
    pub async fn call(&self, context: &ToolContext, call: &ModelToolCall) -> (String, bool) {
        match serde_json::from_str(&call.arguments) {
            Ok(args) => self.call_value(context, &call.name, args).await,
            Err(e) => (format!("arguments are not valid JSON: {e}"), true),
        }
    }

    /// Runs a call with parsed arguments (models, and harnesses through `tool.call`).
    pub async fn call_value(
        &self,
        context: &ToolContext,
        name: &str,
        args: Value,
    ) -> (String, bool) {
        let Some(tool) = self
            .tools
            .iter()
            .find(|t| t.name() == name && context.agent.allows_tool(t.name()))
        else {
            if let Some(parent) = &context.server.parent
                && name.contains("__")
                && context.agent.allows_mcp_tool(name)
            {
                return parent
                    .call_tool(context.session.id(), name, args)
                    .await
                    .unwrap_or_else(|e| (e, true));
            }
            if let Some(result) = context.server.mcp().call(context, name, args).await {
                return result;
            }
            return (format!("unknown tool `{name}`"), true);
        };
        match tool.call(context, args).await {
            Ok(result) => (result, false),
            Err(error) => (error, true),
        }
    }
}
