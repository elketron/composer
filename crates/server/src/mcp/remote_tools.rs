use std::sync::Arc;

use async_trait::async_trait;
use composer_protocol::ToolInfo;
use serde_json::Value;

use crate::mcp::ToolSource;
use crate::worker::ParentLink;

/// A running worker's tools, run by the server over the WebSocket.
pub struct RemoteTools {
    pub link: Arc<ParentLink>,
    pub session: String,
}

#[async_trait]
impl ToolSource for RemoteTools {
    async fn tools(&self) -> Result<Vec<ToolInfo>, String> {
        self.link.tools(&self.session).await
    }

    async fn call(&self, name: &str, args: Value) -> Result<(String, bool), String> {
        self.link.call_tool(&self.session, name, args).await
    }
}
