use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::mcp::ToolSource;
use crate::tools::text_result;

/// Used when the client doesn't say which protocol version it speaks.
const PROTOCOL_VERSION: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Composer's tools for one of its sessions. Pages are addressed as \
site://<site>/<page>; `list_sites` shows the sites and `open site://home` explains the web. \
`set_site` puts you in a site: its tools and MCP servers join yours (the tool list changes).";

/// The JSON-RPC side of MCP: `initialize`, `tools/list`, `tools/call` and `ping`, and the one
/// thing it says unasked, `notifications/tools/list_changed`.
pub struct McpServer {
    tools: Arc<dyn ToolSource>,
    /// Lines it sends without being asked (notifications), to the same writer as replies.
    out: mpsc::UnboundedSender<String>,
    /// The client said `initialize`: notifications may go to it from now on.
    initialized: AtomicBool,
}

impl McpServer {
    /// A server whose notifications go to `out`.
    pub fn new(tools: Arc<dyn ToolSource>, out: mpsc::UnboundedSender<String>) -> Self {
        Self {
            tools,
            out,
            initialized: AtomicBool::new(false),
        }
    }

    /// Tells the client its tool list changed (only once it has initialized).
    pub fn tools_changed(&self) {
        if self.initialized.load(Ordering::SeqCst) {
            let line = json!({ "jsonrpc": "2.0", "method": "notifications/tools/list_changed" });
            let _ = self.out.send(line.to_string());
        }
    }

    /// Passes the tool source's changes on to the client; runs until dropped.
    pub async fn follow_changes(&self) {
        loop {
            self.tools.changed().await;
            self.tools_changed();
        }
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
            "initialize" => {
                self.initialized.store(true, Ordering::SeqCst);
                json!({
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
                "capabilities": { "tools": { "listChanged": true } },
                "serverInfo": { "name": "composer", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
                })
            }
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

    fn server() -> (McpServer, mpsc::UnboundedReceiver<String>) {
        let (out, lines) = mpsc::unbounded_channel();
        (McpServer::new(Arc::new(Fake), out), lines)
    }

    async fn reply(line: Value) -> Value {
        let (server, _lines) = server();
        serde_json::from_str(&server.handle(&line.to_string()).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn initialize_echoes_the_version() {
        let r = reply(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-03-26", "capabilities": {} } }))
        .await;
        assert_eq!(r["id"], 1);
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["capabilities"]["tools"]["listChanged"], true);
    }

    /// A change is pushed, as a notification, once the client has initialized.
    #[tokio::test]
    async fn a_changed_tool_list_is_pushed_after_initialize() {
        let (server, mut lines) = server();
        server.tools_changed();
        assert!(lines.try_recv().is_err(), "not before initialize");
        let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
        server.handle(&init.to_string()).await.unwrap();
        server.tools_changed();
        let line: Value = serde_json::from_str(&lines.try_recv().unwrap()).unwrap();
        assert_eq!(line["method"], "notifications/tools/list_changed");
        assert!(line.get("id").is_none());
    }

    #[tokio::test]
    async fn notifications_get_no_reply() {
        let (server, _lines) = server();
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
        let (server, _lines) = server();
        let r: Value = serde_json::from_str(&server.handle("{nope").await.unwrap()).unwrap();
        assert_eq!(r["error"]["code"], -32700);
    }
}
