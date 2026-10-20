use async_trait::async_trait;
use composer_protocol::ToolInfo;
use serde_json::Value;

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

    /// The output and whether the call failed; `Err` when it could not run at all.
    async fn call(&self, name: &str, args: Value) -> Result<(String, bool), String>;
}
