use std::sync::Arc;

use serde_json::{Value, json};

use crate::mcp::ToolSource;
use crate::tools::text_result;

/// Used when the client doesn't say which protocol version it speaks.
const PROTOCOL_VERSION: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Composer's tools for the ticket you are working on. Pages are \
addressed as site://<site>/<page>; your ticket is a page in site://orchestration/tickets. \
Before you stop, write the ticket's Result section and call finish(done|failed, summary).";

/// The JSON-RPC side of MCP: `initialize`, `tools/list`, `tools/call` and `ping`.
pub struct McpServer {
    tools: Arc<dyn ToolSource>,
}

impl McpServer {
    pub fn new(tools: Arc<dyn ToolSource>) -> Self {
        Self { tools }
    }

    /// Handles one line from the client. Returns the reply line, or `None` for notifications.
    pub async fn handle(&self, line: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(line) {
            Ok(request) => request,
            Err(e) => return Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        // Notifications (`notifications/initialized`, …) have no id and get no reply.
        let id = request.get("id")?.clone();
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let method = request["method"].as_str().unwrap_or_default();
        let result = match method {
            "initialize" => json!({
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "composer", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }),
            "ping" => json!({}),
            "tools/list" => match self.tools.tools().await {
                Ok(tools) => json!({
                    "tools": tools.into_iter().map(|t| json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.parameters,
                    })).collect::<Vec<_>>()
                }),
                Err(e) => return Some(error(id, -32603, &e)),
            },
            "tools/call" => {
                let Some(name) = params["name"].as_str() else {
                    return Some(error(id, -32602, "tools/call needs a name"));
                };
                let args = match &params["arguments"] {
                    Value::Null => json!({}),
                    args => args.clone(),
                };
                // A call that couldn't run is still a tool result, so the model can recover.
                let result = self
                    .tools
                    .call(name, args)
                    .await
                    .unwrap_or_else(|e| text_result(e, true));
                let mut content = vec![json!({ "type": "text", "text": result.text })];
                // Data URLs back to MCP's images (a screenshot reaches Claude Code as one).
                content.extend(result.images.iter().filter_map(|url| {
                    let (mime, data) = url.strip_prefix("data:")?.split_once(";base64,")?;
                    Some(json!({ "type": "image", "data": data, "mimeType": mime }))
                }));
                json!({ "content": content, "isError": result.is_error })
            }
            other => return Some(error(id, -32601, &format!("unknown method `{other}`"))),
        };
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
    }
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use composer_protocol::ToolInfo;

    use super::*;
    use crate::tools::CallResult;

    struct Fake;

    #[async_trait]
    impl ToolSource for Fake {
        async fn tools(&self) -> Result<Vec<ToolInfo>, String> {
            Ok(vec![ToolInfo {
                name: "open".into(),
                description: "Open a page".into(),
                parameters: json!({ "type": "object" }),
            }])
        }

        async fn call(&self, name: &str, args: Value) -> Result<CallResult, String> {
            match name {
                "open" => Ok(text_result(format!("opened {}", args["url"]), false)),
                _ => Err(format!("unknown tool `{name}`")),
            }
        }
    }

    async fn reply(line: Value) -> Value {
        let server = McpServer::new(Arc::new(Fake));
        serde_json::from_str(&server.handle(&line.to_string()).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn initialize_echoes_the_version() {
        let r = reply(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-03-26", "capabilities": {} } }))
        .await;
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert!(r["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn notifications_get_no_reply() {
        let server = McpServer::new(Arc::new(Fake));
        let line = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert_eq!(server.handle(&line.to_string()).await, None);
    }

    #[tokio::test]
    async fn lists_and_calls_tools() {
        let r = reply(json!({ "jsonrpc": "2.0", "id": "a", "method": "tools/list" })).await;
        assert_eq!(r["result"]["tools"][0]["name"], "open");
        assert_eq!(r["result"]["tools"][0]["inputSchema"]["type"], "object");

        let r = reply(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "open", "arguments": { "url": "site://home" } } }))
        .await;
        assert_eq!(r["result"]["content"][0]["text"], "opened \"site://home\"");
        assert_eq!(r["result"]["isError"], false);

        // Failures are results the model sees, not protocol errors.
        let r = reply(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "nope" } }))
        .await;
        assert_eq!(r["result"]["isError"], true);
    }

    #[tokio::test]
    async fn unknown_methods_and_bad_json_are_errors() {
        let r = reply(json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/list" })).await;
        assert_eq!(r["error"]["code"], -32601);
        let server = McpServer::new(Arc::new(Fake));
        let r: Value = serde_json::from_str(&server.handle("{nope").await.unwrap()).unwrap();
        assert_eq!(r["error"]["code"], -32700);
    }
}
