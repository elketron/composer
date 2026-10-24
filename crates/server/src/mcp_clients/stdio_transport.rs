use std::collections::{BTreeMap, HashMap};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::oneshot;

use crate::mcp_clients::types::Transport;

type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// An MCP server as a child process: one JSON-RPC message per line on its stdin and stdout.
/// Its stderr goes to the log. The child is killed when this is dropped.
pub struct StdioTransport {
    name: String,
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    waiting: Waiting,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    tools_changed: Arc<AtomicBool>,
    timeout: Duration,
    _child: Child,
}

impl StdioTransport {
    pub fn spawn(
        name: &str,
        command: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
        timeout: Duration,
    ) -> Result<Self, String> {
        let mut child = Command::new(command)
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("starting `{command}`: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;

        let waiting: Waiting = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let tools_changed = Arc::new(AtomicBool::new(false));
        // Replies to the server's own requests (`ping`, `roots/list`) go through this.
        let (replies, mut outbox) = tokio::sync::mpsc::unbounded_channel::<String>();

        {
            let (waiting, alive, tools_changed, name) = (
                waiting.clone(),
                alive.clone(),
                tools_changed.clone(),
                name.to_string(),
            );
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(message) = serde_json::from_str::<Value>(&line) else {
                        tracing::debug!("mcp {name}: not JSON-RPC: {line}");
                        continue;
                    };
                    handle_message(&message, &waiting, &tools_changed, &replies);
                }
                alive.store(false, Ordering::SeqCst);
                for (_, sender) in waiting.lock().expect("waiting lock").drain() {
                    let _ = sender.send(Err(format!("the MCP server `{name}` exited")));
                }
            });
        }
        {
            let name = name.to_string();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!("mcp {name}: {line}");
                }
            });
        }
        let stdin = Arc::new(tokio::sync::Mutex::new(stdin));
        // Replies to the server share the stdin; once the transport is gone they are dropped.
        let weak = Arc::downgrade(&stdin);
        tokio::spawn(async move {
            while let Some(line) = outbox.recv().await {
                let Some(stdin) = weak.upgrade() else { return };
                let _ = write_line(&stdin, &line).await;
            }
        });
        Ok(Self {
            name: name.to_string(),
            stdin,
            waiting,
            next_id: AtomicU64::new(1),
            alive,
            tools_changed,
            timeout,
            _child: child,
        })
    }

    async fn send(&self, message: &Value) -> Result<(), String> {
        if !self.alive() {
            return Err(format!("the MCP server `{}` exited", self.name));
        }
        write_line(&self.stdin, &message.to_string()).await
    }
}

async fn write_line(stdin: &tokio::sync::Mutex<ChildStdin>, line: &str) -> Result<(), String> {
    let mut stdin = stdin.lock().await;
    stdin
        .write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stdin.flush().await.map_err(|e| e.to_string())
}

/// A response settles its waiting request; a request from the server gets a reply; a
/// notification may say the tools changed.
fn handle_message(
    message: &Value,
    waiting: &Waiting,
    tools_changed: &AtomicBool,
    replies: &tokio::sync::mpsc::UnboundedSender<String>,
) {
    let method = message.get("method").and_then(Value::as_str);
    match (message.get("id"), method) {
        (Some(id), None) => {
            let Some(id) = id.as_u64() else { return };
            let Some(sender) = waiting.lock().expect("waiting lock").remove(&id) else {
                return;
            };
            let _ = sender.send(match message.get("error") {
                Some(error) => Err(error["message"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| error.to_string())),
                None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
            });
        }
        (Some(id), Some(method)) => {
            let reply = match method {
                "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
                "roots/list" => json!({ "jsonrpc": "2.0", "id": id, "result": { "roots": [] } }),
                _ => json!({ "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32601, "message": format!("composer doesn't support `{method}`") } }),
            };
            let _ = replies.send(reply.to_string());
        }
        (None, Some("notifications/tools/list_changed")) => {
            tools_changed.store(true, Ordering::SeqCst);
        }
        _ => {}
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = oneshot::channel();
        self.waiting
            .lock()
            .expect("waiting lock")
            .insert(id, sender);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(e) = self.send(&message).await {
            self.waiting.lock().expect("waiting lock").remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(self.timeout, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(format!("the MCP server `{}` exited", self.name)),
            Err(_) => {
                self.waiting.lock().expect("waiting lock").remove(&id);
                let cancel = json!({ "jsonrpc": "2.0", "method": "notifications/cancelled",
                    "params": { "requestId": id, "reason": "timed out" } });
                let _ = self.send(&cancel).await;
                Err(format!(
                    "`{method}` on `{}` timed out after {}s",
                    self.name,
                    self.timeout.as_secs()
                ))
            }
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn take_tools_changed(&self) -> bool {
        self.tools_changed.swap(false, Ordering::SeqCst)
    }
}
