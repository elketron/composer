use std::sync::Arc;

use composer_protocol::ToolInfo;
use serde_json::Value;

use crate::agent::{AgentDefinition, Site};
use crate::mcp_clients::McpServers;
use crate::provider::{ModelToolCall, ToolSpec};
use crate::server::Server;
use crate::tools::actions_tool::ActionsTool;
use crate::tools::answer_approval_tool::AnswerApprovalTool;
use crate::tools::continue_worker_tool::ContinueWorkerTool;
use crate::tools::desk_tool::DeskTool;
use crate::tools::edit_tool::EditTool;
use crate::tools::enter_site_tool::EnterSiteTool;
use crate::tools::finish_tool::FinishTool;
use crate::tools::hunk_tool::HunkTool;
use crate::tools::leave_site_tool::LeaveSiteTool;
use crate::tools::list_sites_tool::ListSitesTool;
use crate::tools::merge_ticket_tool::MergeTicketTool;
use crate::tools::new_app_tool::NewAppTool;
use crate::tools::new_project_tool::NewProjectTool;
use crate::tools::new_site_tool::NewSiteTool;
use crate::tools::open_tool::OpenTool;
use crate::tools::publish_site_tool::PublishSiteTool;
use crate::tools::run_tool::RunTool;
use crate::tools::search_tool::SearchTool;
use crate::tools::selection_tool::SelectionTool;
use crate::tools::set_project_tool::SetProjectTool;
use crate::tools::show_app_tool::ShowAppTool;
use crate::tools::show_page_tool::ShowPageTool;
use crate::tools::spawn_worker_tool::SpawnWorkerTool;
use crate::tools::stop_worker_tool::StopWorkerTool;
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
                Arc::new(SetProjectTool),
                Arc::new(EnterSiteTool),
                Arc::new(LeaveSiteTool),
                Arc::new(ActionsTool),
                Arc::new(RunTool),
                Arc::new(NewSiteTool),
                Arc::new(NewAppTool),
                Arc::new(NewProjectTool),
                Arc::new(SpawnWorkerTool),
                Arc::new(MergeTicketTool),
                Arc::new(PublishSiteTool),
                Arc::new(ContinueWorkerTool),
                Arc::new(StopWorkerTool),
                Arc::new(AnswerApprovalTool),
                Arc::new(WatchTool),
                Arc::new(ShowAppTool),
                Arc::new(ShowPageTool),
                Arc::new(DeskTool),
                Arc::new(SelectionTool),
                Arc::new(TerminalReadTool),
                Arc::new(TerminalSendTool),
                Arc::new(HunkTool),
                Arc::new(FinishTool),
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
    /// A worker process gets its MCP tools from the parent, which runs them: one process per
    /// MCP server, and approvals where the clients are.
    pub async fn specs_with_mcp(
        &self,
        server: &Server,
        session: &str,
        agent: &AgentDefinition,
        site: Option<&Site>,
    ) -> Vec<ToolSpec> {
        let mut specs = self.specs_for(agent, site);
        match &server.parent {
            Some(parent)
                if !agent.mcp_servers().is_empty()
                    || site.is_some_and(|site| !site.mcp.is_empty()) =>
            {
                match parent.tools(session).await {
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
                }
            }
            Some(_) => {}
            None => specs.extend(server.mcp().specs_for(agent, site).await),
        }
        specs
    }

    /// The same, for the protocol (`tool.list`, answered by the parent).
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
        match serde_json::from_str(&call.arguments) {
            Ok(args) => self.call_value(context, &call.name, args).await,
            Err(e) => text_result(format!("arguments are not valid JSON: {e}"), true),
        }
    }

    /// Runs a call with parsed arguments (models, and harnesses through `tool.call`).
    pub async fn call_value(&self, context: &ToolContext, name: &str, args: Value) -> CallResult {
        let Some(tool) = self.tools.iter().find(|t| {
            t.name() == name && Self::allowed(&context.agent, context.site.as_deref(), t.name())
        }) else {
            if let Some(parent) = &context.server.parent
                && name.contains("__")
                && context
                    .server
                    .mcp()
                    .allows(&context.agent, context.site.as_deref(), name)
            {
                return parent
                    .call_tool(context.session.id(), name, args)
                    .await
                    .unwrap_or_else(|e| text_result(e, true));
            }
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
