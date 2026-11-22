use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::mcp_clients::EVENT_METHOD;
use crate::mcp_clients::types::{EventSink, Transport};

const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";

/// An MCP server over streamable HTTP: each message is a POST; a request's reply is a JSON body
/// or an event stream that carries it. The server's session id (from `initialize`) and the
/// negotiated protocol version go with every later message. What the server sends on its own
/// comes on a GET event stream ([`Transport::listen`]), kept open while this lives.
pub struct HttpTransport {
    name: String,
    url: String,
    client: reqwest::Client,
    headers: HeaderMap,
    session: Mutex<Option<String>>,
    version: Mutex<Option<String>>,
    next_id: AtomicU64,
    alive: AtomicBool,
    tools_changed: Arc<AtomicBool>,
    timeout: Duration,
    on_event: EventSink,
    /// Ends the GET stream when this is dropped.
    stop: CancellationToken,
}

impl HttpTransport {
    pub fn new(
        name: &str,
        url: &str,
        headers: &BTreeMap<String, String>,
        timeout: Duration,
        on_event: EventSink,
    ) -> Result<Self, String> {
        let mut map = HeaderMap::new();
        for (key, value) in headers {
            let key = HeaderName::from_bytes(key.as_bytes())
                .map_err(|e| format!("header `{key}`: {e}"))?;
            let value =
                HeaderValue::from_str(value).map_err(|e| format!("header `{key}`'s value: {e}"))?;
            map.insert(key, value);
        }
        Ok(Self {
            name: name.to_string(),
            url: url.to_string(),
            client: reqwest::Client::new(),
            headers: map,
            session: Mutex::new(None),
            version: Mutex::new(None),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            tools_changed: Arc::default(),
            timeout,
            on_event,
            stop: CancellationToken::new(),
        })
    }

    async fn post(&self, message: &Value) -> Result<reqwest::Response, String> {
        let mut request = self
            .client
            .post(&self.url)
            .headers(self.headers.clone())
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .timeout(self.timeout)
            .body(message.to_string());
        if let Some(session) = self.session.lock().expect("session lock").clone() {
            request = request.header(SESSION_HEADER, session);
        }
        if let Some(version) = self.version.lock().expect("version lock").clone() {
            request = request.header(VERSION_HEADER, version);
        }
        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                format!(
                    "`{}` timed out after {}s",
                    self.name,
                    self.timeout.as_secs()
                )
            } else {
                format!("reaching `{}`: {e}", self.name)
            }
        })?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND && self.session.lock().expect("lock").is_some()
        {
            // The server forgot the session: connect again from scratch.
            self.alive.store(false, Ordering::SeqCst);
            return Err(format!(
                "`{}` ended the session; it reconnects on the next use",
                self.name
            ));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            // Its token may have changed (a restart): the next use connects again, which reads
            // `bearer_file` again.
            self.alive.store(false, Ordering::SeqCst);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let body: String = body.chars().take(300).collect();
            return Err(format!("`{}` answered {status}: {body}", self.name));
        }
        if let Some(session) = response
            .headers()
            .get(SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock().expect("session lock") = Some(session.to_string());
        }
        Ok(response)
    }

    /// The reply to request `id` from an event stream; notifications on the way are noted.
    async fn reply_from_stream(
        &self,
        response: reqwest::Response,
        id: u64,
    ) -> Result<Value, String> {
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("reading `{}`: {e}", self.name))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
            for message in stream_messages(&mut buffer) {
                if let Some(reply) = self.reply_to(&message, id) {
                    return reply;
                }
            }
        }
        Err(format!(
            "`{}` closed the stream without answering",
            self.name
        ))
    }

    fn reply_to(&self, message: &Value, id: u64) -> Option<Result<Value, String>> {
        notified(message, &self.tools_changed, &self.on_event);
        if message.get("id").and_then(Value::as_u64) != Some(id) || message.get("method").is_some()
        {
            return None;
        }
        Some(match message.get("error") {
            Some(error) => Err(error["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string())),
            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
        })
    }
}

impl Drop for HttpTransport {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// The JSON-RPC messages of the complete events at the front of `buffer` (taken out of it).
fn stream_messages(buffer: &mut String) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Some(end) = buffer.find("\n\n") {
        let event: String = buffer.drain(..end + 2).collect();
        let data: Vec<&str> = event
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect();
        if data.is_empty() {
            continue;
        }
        if let Ok(message) = serde_json::from_str::<Value>(&data.join("\n")) {
            messages.push(message);
        }
    }
    messages
}

/// A notification the server sent: its tools changed, or an event for a session.
fn notified(message: &Value, tools_changed: &AtomicBool, on_event: &EventSink) {
    if message.get("id").is_some() {
        return;
    }
    match message.get("method").and_then(Value::as_str) {
        Some("notifications/tools/list_changed") => tools_changed.store(true, Ordering::SeqCst),
        Some(EVENT_METHOD) => on_event(message.get("params").cloned().unwrap_or(Value::Null)),
        _ => {}
    }
}

/// The server's GET event stream, until `stop`: what it sends there is noted like what comes
/// with replies. A server without one (405) or one that refuses is left alone; a stream that
/// ends is opened again.
async fn listen(
    name: String,
    request: reqwest::RequestBuilder,
    tools_changed: Arc<AtomicBool>,
    on_event: EventSink,
    stop: CancellationToken,
) {
    loop {
        let Some(attempt) = request.try_clone() else {
            return;
        };
        let response = tokio::select! {
            _ = stop.cancelled() => return,
            response = attempt.send() => response,
        };
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                tracing::debug!("mcp {name}: no event stream ({})", response.status());
                return;
            }
            Err(e) => {
                tracing::debug!("mcp {name}: no event stream: {e}");
                return;
            }
        };
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        loop {
            let chunk = tokio::select! {
                _ = stop.cancelled() => return,
                chunk = stream.next() => chunk,
            };
            let Some(Ok(chunk)) = chunk else { break };
            buffer.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
            for message in stream_messages(&mut buffer) {
                notified(&message, &tools_changed, &on_event);
            }
        }
        tokio::select! {
            _ = stop.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let response = self.post(&message).await?;
        let is_stream = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let result = if is_stream {
            self.reply_from_stream(response, id).await
        } else {
            let body: Value = response
                .json()
                .await
                .map_err(|e| format!("`{}` sent no JSON: {e}", self.name))?;
            // A batch is allowed; take ours.
            let messages = match body {
                Value::Array(items) => items,
                single => vec![single],
            };
            messages
                .iter()
                .find_map(|m| self.reply_to(m, id))
                .unwrap_or_else(|| Err(format!("`{}` sent no reply to `{method}`", self.name)))
        }?;
        if method == "initialize"
            && let Some(version) = result["protocolVersion"].as_str()
        {
            *self.version.lock().expect("version lock") = Some(version.to_string());
        }
        Ok(result)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.post(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await
            .map(|_| ())
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn take_tools_changed(&self) -> bool {
        self.tools_changed.swap(false, Ordering::SeqCst)
    }

    fn listen(&self) {
        let mut request = self
            .client
            .get(&self.url)
            .headers(self.headers.clone())
            .header(ACCEPT, "text/event-stream");
        if let Some(session) = self.session.lock().expect("session lock").clone() {
            request = request.header(SESSION_HEADER, session);
        }
        if let Some(version) = self.version.lock().expect("version lock").clone() {
            request = request.header(VERSION_HEADER, version);
        }
        tokio::spawn(listen(
            self.name.clone(),
            request,
            self.tools_changed.clone(),
            self.on_event.clone(),
            self.stop.clone(),
        ));
    }
}
