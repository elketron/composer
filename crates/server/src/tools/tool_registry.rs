use std::sync::Arc;

use composer_protocol::{RunKind, ToolInfo};
use serde_json::Value;

use crate::agent::Site;
use crate::mcp_clients::McpServers;
use crate::provider::{ModelToolCall, ToolSpec};
use crate::server::Server;
use crate::tools::actions_tool::ActionsTool;
use crate::tools::arg_repair::RepairedArgs;
use crate::tools::desk_tool::DeskTool;
use crate::tools::edit_tool::EditTool;
use crate::tools::finish_tool::FinishTool;
use crate::tools::list_sites_tool::ListSitesTool;
use crate::tools::new_site_tool::NewSiteTool;
use crate::tools::open_tool::OpenTool;
use crate::tools::run_tool::RunTool;
use crate::tools::search_tool::SearchTool;
use crate::tools::set_site_tool::SetSiteTool;
use crate::tools::show_page_tool::ShowPageTool;
use crate::tools::spawn_run_tool::SpawnRunTool;
use crate::tools::terminal_read_tool::TerminalReadTool;
use crate::tools::terminal_send_tool::TerminalSendTool;
use crate::tools::watch_tool::WatchTool;
use crate::tools::write_tool::WriteTool;
use crate::tools::{CallResult, Tool, ToolContext, text_result};
use crate::watches::WRITER;

/// The tools a workflow run has: it reads, it does not change anything.
pub const READ_ONLY_TOOLS: [&str; 5] = ["open", "search", "list_sites", "set_site", "finish"];

/// Native tools only a site brings, when its `tools:` lists them (Home's `new_site`). Every other
/// native tool is an entry tool: there in any site, and in none.
pub const SITE_TOOLS: [&str; 1] = ["new_site"];

/// All tools the server provides. A run gets the entry tools and the tools its site lists,
/// as far as its kind allows.
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
                Arc::new(SetSiteTool),
                Arc::new(FinishTool),
                Arc::new(ActionsTool),
                Arc::new(RunTool),
                Arc::new(NewSiteTool),
                Arc::new(WatchTool),
                Arc::new(ShowPageTool),
                Arc::new(DeskTool),
                Arc::new(TerminalReadTool),
                Arc::new(TerminalSendTool),
                Arc::new(SpawnRunTool),
            ],
        }
    }

    /// The native tools a run of `kind` in `site` has, narrowed to `only` when given.
    pub fn specs_for(
        &self,
        kind: RunKind,
        site: Option<&Site>,
        only: Option<&[&str]>,
    ) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|t| Self::allowed(kind, site, only, t.name()))
            .map(|t| ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect()
    }

    /// Whether a run of `kind` has the native tool `tool`: none for a single shot, the
    /// read-only ones for a workflow run, all but `finish` for an agentic one.
    pub fn kind_allows(kind: RunKind, tool: &str) -> bool {
        match kind {
            RunKind::SingleShot => false,
            RunKind::Workflow => READ_ONLY_TOOLS.contains(&tool),
            RunKind::Agentic => tool != "finish",
        }
    }

    /// An entry tool, or one `site` lists; and one the kind allows, and `only` names.
    fn allowed(kind: RunKind, site: Option<&Site>, only: Option<&[&str]>, tool: &str) -> bool {
        let offered =
            !SITE_TOOLS.contains(&tool) || site.is_some_and(|site| site.allows_tool(tool));
        offered && Self::kind_allows(kind, tool) && only.is_none_or(|only| only.contains(&tool))
    }

    /// The native tools a run of `kind` has, then the MCP tools of `site` it has (servers it
    /// can reach).
    pub async fn specs_with_mcp(
        &self,
        server: &Server,
        kind: RunKind,
        site: Option<&Site>,
    ) -> Vec<ToolSpec> {
        let mut specs = self.specs_for(kind, site, None);
        specs.extend(server.mcp().specs_for(kind, site).await);
        specs
    }

    /// The same, for the protocol (`tool.list`).
    pub async fn infos_for(
        &self,
        mcp: &McpServers,
        kind: RunKind,
        site: Option<&Site>,
    ) -> Vec<ToolInfo> {
        let mut specs = self.specs_for(kind, site, None);
        specs.extend(mcp.specs_for(kind, site).await);
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
            t.name() == name
                && Self::allowed(
                    context.kind,
                    context.site.as_deref(),
                    context.only,
                    t.name(),
                )
        }) else {
            if let Some(result) = context.server.mcp().call(context, name, args).await {
                return result;
            }
            return text_result(format!("unknown tool `{name}`"), true);
        };
        // Its own writes don't wake its watches.
        WRITER
            .scope(
                context.run.id().to_string(),
                tool.call_result(context, args),
            )
            .await
    }
}
