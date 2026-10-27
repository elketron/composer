use async_trait::async_trait;
use composer_protocol::ToolInfo;
use serde_json::Value;

use crate::tools::CallResult;

/// What the parent passes on the command line.
#[derive(Debug, Clone)]
pub struct McpOptions {
    /// The running worker session whose tools are offered.
    pub session: String,
    pub parent_url: String,
}

/// Where the MCP server's tools come from.
#[async_trait]
pub trait ToolSource: Send + Sync {
    async fn tools(&self) -> Result<Vec<ToolInfo>, String>;

    /// The output, whether the call failed and its images; `Err` when it could not run at all.
    async fn call(&self, name: &str, args: Value) -> Result<CallResult, String>;
}
