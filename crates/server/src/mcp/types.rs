use async_trait::async_trait;
use composer_protocol::ToolInfo;
use serde_json::Value;

use crate::tools::CallResult;

/// What `composer mcp` is given on the command line.
#[derive(Debug, Clone)]
pub struct McpOptions {
    /// The session whose tools are offered.
    pub session: String,
    /// The running server's address (`http://127.0.0.1:7420`).
    pub server_url: String,
}

/// Where the MCP server's tools come from.
#[async_trait]
pub trait ToolSource: Send + Sync {
    async fn tools(&self) -> Result<Vec<ToolInfo>, String>;

    /// The output, whether the call failed and its images; `Err` when it could not run at all.
    async fn call(&self, name: &str, args: Value) -> Result<CallResult, String>;

    /// Resolves when the tools changed since the last time it resolved (the session's site was
    /// set). One that never changes never resolves.
    async fn changed(&self) {
        std::future::pending::<()>().await
    }
}
