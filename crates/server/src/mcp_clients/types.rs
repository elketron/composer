use async_trait::async_trait;
use serde_json::Value;

/// A tool as an MCP server lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// How JSON-RPC messages reach a server: a child's stdio, or streamable HTTP.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends a request and waits for its result (`Err`: the error, or the transport failed).
    async fn request(&self, method: &str, params: Value) -> Result<Value, String>;

    /// Sends a notification (no reply).
    async fn notify(&self, method: &str, params: Value) -> Result<(), String>;

    /// Whether it can still carry messages (a stdio child that exited can't).
    fn alive(&self) -> bool;

    /// Whether the server said its tools changed since the last call (and clears it).
    fn take_tools_changed(&self) -> bool;
}

/// `stdio` or `http`.
pub fn transport_kind(command: &str) -> &'static str {
    if command.is_empty() { "http" } else { "stdio" }
}

/// One server's connection, or why it has none.
#[derive(Default)]
pub struct Slot {
    pub client: Option<std::sync::Arc<crate::mcp_clients::McpClient>>,
    pub error: Option<String>,
    pub failed_at: Option<std::time::Instant>,
}

/// `pattern` is a name, `prefix*`, or `*`.
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

/// A tool name models accept (`[A-Za-z0-9_-]`): anything else becomes `_`.
pub fn tool_name(server: &str, tool: &str) -> String {
    let clean: String = tool
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{server}__{clean}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_and_names() {
        assert!(glob_matches("github__*", "github__get_issue"));
        assert!(glob_matches("get_*", "get_issue"));
        assert!(glob_matches("*", "anything"));
        assert!(!glob_matches("get_issue", "get_issues"));
        assert_eq!(tool_name("fs", "read.file/v2"), "fs__read_file_v2");
    }
}
