use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};

use crate::McpServerConfig;
use crate::mcp_clients::http_transport::HttpTransport;
use crate::mcp_clients::stdio_transport::StdioTransport;
use crate::mcp_clients::types::{RemoteTool, Transport};
use crate::sandbox::SandboxSpec;
use crate::tools::CallResult;
use crate::tools::expand_home;

const PROTOCOL_VERSION: &str = "2025-06-18";
/// A tool call may take a while (a build, a search); the handshake may not.
const CALL_TIMEOUT: Duration = Duration::from_secs(300);
/// Output beyond this is cut before it reaches the model.
const MAX_OUTPUT: usize = 100 * 1024;

/// One connected MCP server: initialized, with its tools listed.
pub struct McpClient {
    transport: Box<dyn Transport>,
    tools: std::sync::Mutex<Vec<RemoteTool>>,
}

impl McpClient {
    /// Starts or reaches the server, does the handshake and lists its tools. A stdio server
    /// starts in `sandbox` (with its own `binds_*` added), in the workspace's sites (or its `cwd`), with
    /// `workspace`'s variables set (its own `env` on top).
    pub async fn connect(
        name: &str,
        config: &McpServerConfig,
        workspace: &BTreeMap<String, String>,
        sandbox: &SandboxSpec,
    ) -> Result<Self, String> {
        if let Some(problem) = &config.problem {
            return Err(problem.clone());
        }
        let transport: Box<dyn Transport> = if !config.command.is_empty() {
            let mut env = workspace.clone();
            for (key, value) in &config.env {
                env.insert(key.clone(), expand(value, workspace)?);
            }
            let mut spec = sandbox.clone();
            for bind in &config.binds_ro {
                spec = spec.with_ro(expand_home(&expand(bind, workspace)?));
            }
            for bind in &config.binds_rw {
                spec = spec.with_rw(expand_home(&expand(bind, workspace)?));
            }
            let dir = if config.cwd.is_empty() {
                spec.sites().to_path_buf()
            } else {
                let cwd = expand_home(&expand(&config.cwd, workspace)?);
                // A bundle's code is bound read-only (its own `binds_ro`); it writes elsewhere.
                if !config.cwd_read_only {
                    spec = spec.with_rw(cwd.clone());
                }
                cwd
            };
            let mut command = spec.in_dir(dir).command(&config.command)?;
            command.args(&config.args).envs(&env);
            Box::new(StdioTransport::spawn(
                name,
                &config.command,
                command,
                CALL_TIMEOUT,
            )?)
        } else if !config.url.is_empty() {
            let mut headers: BTreeMap<String, String> = config
                .headers
                .iter()
                .map(|(k, v)| Ok((k.clone(), expand(v, workspace)?)))
                .collect::<Result<_, String>>()?;
            // `bearer_file`: read now, at every connect (the server may have restarted with a new
            // token; a 401 ends the connection, so the next use connects and reads again). An
            // `Authorization` the user wrote in `headers` wins.
            let own_authorization = headers
                .keys()
                .any(|name| name.eq_ignore_ascii_case("authorization"));
            if !config.bearer_file.trim().is_empty() && !own_authorization {
                let file = expand_home(&expand(config.bearer_file.trim(), workspace)?);
                let token = crate::viewer_upstream::read_bearer(&file)?;
                headers.insert("Authorization".into(), format!("Bearer {token}"));
            }
            Box::new(HttpTransport::new(
                name,
                &config.url,
                &headers,
                CALL_TIMEOUT,
            )?)
        } else {
            return Err("needs a `command` (stdio) or a `url` (HTTP)".into());
        };
        let init = tokio::time::timeout(
            Duration::from_secs(30),
            transport.request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "composer", "version": env!("CARGO_PKG_VERSION") },
                }),
            ),
        )
        .await
        .map_err(|_| "no answer to `initialize` within 30s".to_string())??;
        if init
            .get("capabilities")
            .and_then(|c| c.get("tools"))
            .is_none()
        {
            tracing::info!("mcp {name}: the server offers no tools");
        }
        transport
            .notify("notifications/initialized", json!({}))
            .await?;
        let client = Self {
            transport,
            tools: std::sync::Mutex::default(),
        };
        client.refresh().await?;
        Ok(client)
    }

    pub fn alive(&self) -> bool {
        self.transport.alive()
    }

    /// Its tools, listed again first when the server said they changed.
    pub async fn tools(&self) -> Vec<RemoteTool> {
        if self.transport.take_tools_changed()
            && let Err(e) = self.refresh().await
        {
            tracing::warn!("listing MCP tools again: {e}");
        }
        self.tools.lock().expect("tools lock").clone()
    }

    async fn refresh(&self) -> Result<(), String> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let page = self.transport.request("tools/list", params).await?;
            for tool in page["tools"].as_array().into_iter().flatten() {
                let Some(name) = tool["name"].as_str() else {
                    continue;
                };
                tools.push(RemoteTool {
                    name: name.to_string(),
                    description: tool["description"]
                        .as_str()
                        .or(tool["title"].as_str())
                        .unwrap_or_default()
                        .to_string(),
                    input_schema: match &tool["inputSchema"] {
                        Value::Object(_) => tool["inputSchema"].clone(),
                        _ => json!({ "type": "object" }),
                    },
                });
            }
            cursor = page["nextCursor"]
                .as_str()
                .filter(|c| !c.is_empty())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        *self.tools.lock().expect("tools lock") = tools;
        Ok(())
    }

    /// Calls a tool: its output as text, whether it failed, and its images.
    pub async fn call(&self, tool: &str, args: Value) -> Result<CallResult, String> {
        let result = self
            .transport
            .request("tools/call", json!({ "name": tool, "arguments": args }))
            .await?;
        Ok(CallResult {
            text: output_text(&result),
            is_error: result["isError"].as_bool().unwrap_or(false),
            images: output_images(&result),
        })
    }
}

/// The result's images as data URLs (they go to the model beside its text).
pub fn output_images(result: &Value) -> Vec<String> {
    result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "image")
        .filter_map(|item| {
            let mime = item["mimeType"].as_str().unwrap_or("image/png");
            Some(format!("data:{mime};base64,{}", item["data"].as_str()?))
        })
        .collect()
}

/// The result's content as text: text items joined; images named (they go along as images,
/// [`output_images`]), audio and resources named, not inlined; `structuredContent` when there
/// is nothing else.
pub fn output_text(result: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    for item in result["content"].as_array().into_iter().flatten() {
        let part = match item["type"].as_str().unwrap_or_default() {
            "text" => item["text"].as_str().unwrap_or_default().to_string(),
            "image" => format!(
                "[image ({})]",
                item["mimeType"].as_str().unwrap_or("unknown type")
            ),
            "audio" => format!(
                "[audio ({}) not shown]",
                item["mimeType"].as_str().unwrap_or("unknown type")
            ),
            "resource" => match item["resource"]["text"].as_str() {
                Some(text) => text.to_string(),
                None => format!(
                    "[resource {} not shown]",
                    item["resource"]["uri"].as_str().unwrap_or_default()
                ),
            },
            "resource_link" => format!(
                "[link: {} {}]",
                item["name"].as_str().unwrap_or_default(),
                item["uri"].as_str().unwrap_or_default()
            ),
            other => format!("[{other} content not shown]"),
        };
        parts.push(part);
    }
    let mut text = parts.join("\n");
    if text.trim().is_empty()
        && let Some(structured) = result.get("structuredContent")
    {
        text = serde_json::to_string_pretty(structured).unwrap_or_default();
    }
    if text.trim().is_empty() {
        text = "(no output)".into();
    }
    if text.len() > MAX_OUTPUT {
        let mut end = MAX_OUTPUT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n… (cut: the output was over 100 KB)");
    }
    text
}

/// `${VAR}` replaced from `workspace`'s variables, else composer's environment; an unset variable
/// is an error, not "".
pub fn expand(value: &str, workspace: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return Ok(out);
        };
        let var = &after[..end];
        let value = workspace
            .get(var)
            .cloned()
            .map_or_else(|| std::env::var(var), Ok)
            .map_err(|_| format!("`${{{var}}}` is not set in composer's environment"))?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_variables() {
        // SAFETY: the test sets a variable no other test reads.
        unsafe { std::env::set_var("COMPOSER_MCP_TEST_TOKEN", "abc") };
        assert_eq!(
            expand("Bearer ${COMPOSER_MCP_TEST_TOKEN}", &BTreeMap::new()).unwrap(),
            "Bearer abc"
        );
        assert_eq!(expand("plain", &BTreeMap::new()).unwrap(), "plain");
        assert!(expand("${COMPOSER_MCP_SURELY_UNSET}", &BTreeMap::new()).is_err());
    }

    #[test]
    fn outputs() {
        let r = json!({ "content": [
            { "type": "text", "text": "one" },
            { "type": "image", "data": "AAAA", "mimeType": "image/png" },
            { "type": "resource", "resource": { "uri": "file:///a", "text": "two" } },
        ] });
        assert_eq!(output_text(&r), "one\n[image (image/png)]\ntwo");
        assert_eq!(output_images(&r), ["data:image/png;base64,AAAA"]);
        let r = json!({ "content": [], "structuredContent": { "n": 1 } });
        assert_eq!(output_text(&r), "{\n  \"n\": 1\n}");
        assert_eq!(output_text(&json!({})), "(no output)");
    }
}
