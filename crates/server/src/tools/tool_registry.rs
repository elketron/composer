use std::sync::Arc;

use crate::agent::AgentDefinition;
use crate::provider::{ModelToolCall, ToolSpec};
use crate::tools::actions_tool::ActionsTool;
use crate::tools::edit_tool::EditTool;
use crate::tools::finish_tool::FinishTool;
use crate::tools::list_sites_tool::ListSitesTool;
use crate::tools::new_app_tool::NewAppTool;
use crate::tools::open_tool::OpenTool;
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
                Arc::new(NewAppTool),
                Arc::new(SpawnWorkerTool),
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

    /// Runs a call. Returns the result text and whether it failed.
    pub async fn call(&self, context: &ToolContext, call: &ModelToolCall) -> (String, bool) {
        let Some(tool) = self
            .tools
            .iter()
            .find(|t| t.name() == call.name && context.agent.allows_tool(t.name()))
        else {
            return (format!("unknown tool `{}`", call.name), true);
        };
        let args = match serde_json::from_str(&call.arguments) {
            Ok(args) => args,
            Err(e) => return (format!("arguments are not valid JSON: {e}"), true),
        };
        match tool.call(context, args).await {
            Ok(result) => (result, false),
            Err(error) => (error, true),
        }
    }
}
