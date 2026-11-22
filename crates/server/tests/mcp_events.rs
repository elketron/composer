//! Events from MCP servers: every `tools/call` names the calling session in `_meta`, and a
//! server's `notifications/composer/event` lands in a session as an automatic notice: the one
//! it names, else the one that called it last, if that session has the server's tools;
//! otherwise it is dropped. Bursts are coalesced. Over stdio (a fake server in Python) and
//! streamable HTTP (its GET event stream).

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use composer_protocol::{ApprovalDecision, MessageSource, RunKind, ServerMessage};
use composer_server::agent::{SiteSet, set_site};
use composer_server::session::Session;
use composer_server::tools::ToolContext;
use composer_server::{Config, McpServerConfig, Server};
use serde_json::{Value, json};
use support::*;
use tokio_util::sync::CancellationToken;

/// Tool `emit`: notes the call's `_meta` in `meta.jsonl` next to the script, then sends each of
/// `events` (after `delay` seconds, from a thread, when given) and answers.
const FAKE_SERVER: &str = r#"
import json, os, sys, threading, time
here = os.path.dirname(os.path.abspath(__file__))
lock = threading.Lock()
def send(message):
    with lock:
        sys.stdout.write(json.dumps(message) + "\n")
        sys.stdout.flush()
def emit(events, delay):
    if delay:
        time.sleep(delay)
    for e in events:
        send({"jsonrpc": "2.0", "method": "notifications/composer/event", "params": e})
for line in sys.stdin:
    m = json.loads(line)
    method, mid, p = m.get("method"), m.get("id"), m.get("params") or {}
    if mid is None:
        continue
    if method == "initialize":
        r = {"protocolVersion": p["protocolVersion"], "capabilities": {"tools": {}},
             "serverInfo": {"name": "fake", "version": "1"}}
    elif method == "tools/list":
        r = {"tools": [{"name": "emit", "description": "Sends events", "inputSchema": {"type": "object"}}]}
    elif method == "tools/call":
        with open(os.path.join(here, "meta.jsonl"), "a") as f:
            f.write(json.dumps(p.get("_meta")) + "\n")
        args = p.get("arguments") or {}
        events, delay = args.get("events", []), args.get("delay", 0)
        if delay:
            threading.Thread(target=emit, args=(events, delay)).start()
        else:
            emit(events, 0)
        r = {"content": [{"type": "text", "text": "ok"}]}
    else:
        r = {}
    send({"jsonrpc": "2.0", "id": mid, "result": r})
"#;

fn has_python() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn fake(script: &std::path::Path) -> McpServerConfig {
    McpServerConfig {
        command: "python3".into(),
        args: vec![script.display().to_string()],
        allow: vec!["*".into()],
        enabled: true,
        ..Default::default()
    }
}

/// The `_meta` of each call the fake server got.
fn metas(scratch: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(scratch.join("meta.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

async fn call(server: &Arc<Server>, session: &Session, tool: &str, args: Value) {
    let context = ToolContext::for_session(server, session.id()).unwrap();
    let result = server.tools.call_value(&context, tool, args).await;
    assert!(!result.is_error, "{}", result.text);
}

/// The MCP notices sessions got: `(session, server, text)`, collected for `wait`.
async fn notices(
    events: &mut tokio::sync::broadcast::Receiver<Arc<ServerMessage>>,
    wait: Duration,
) -> Vec<(String, String, String)> {
    let mut got = Vec::new();
    let deadline = tokio::time::Instant::now() + wait;
    while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, events.recv()).await {
        if let ServerMessage::MessageDone {
            session, message, ..
        } = &*event
            && let Some(MessageSource::Mcp { server }) = &message.source
        {
            got.push((session.clone(), server.clone(), message.content.clone()));
        }
    }
    got
}

#[tokio::test]
async fn events_reach_the_session_the_server_picks() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let script = scratch.path().join("fake_mcp.py");
    std::fs::write(&script, FAKE_SERVER).unwrap();
    let mut config = Config::defaults();
    config.mcp.insert("fake".into(), fake(&script));
    let (_dir, server, _provider, _addr) = start_with(config, vec![]).await;
    let mut events = server.hub.subscribe();
    let sessions = &server.sessions;
    let a = sessions.create(RunKind::Agentic, None, None).unwrap();
    let b = sessions.create(RunKind::Agentic, None, None).unwrap();
    let single = sessions.create(RunKind::SingleShot, None, None).unwrap();

    // Named: B's call sends an event for A. The call said who called.
    let event = json!({ "session": a.id(), "text": "clicked Save", "data": { "board": 2 } });
    call(&server, &b, "fake__emit", json!({ "events": [event] })).await;
    assert_eq!(
        metas(scratch.path()),
        vec![json!({ "composer/session": b.id() })]
    );
    let got = notices(&mut events, Duration::from_millis(500)).await;
    assert_eq!(
        got,
        vec![(
            a.id().to_string(),
            "fake".to_string(),
            "[automatic notice: fake event] clicked Save\n  {\"board\":2}".to_string()
        )]
    );

    // Unnamed: the session that called last gets it. A's call sends it later; B calls
    // meanwhile.
    let later = json!({ "text": "dragged a box" });
    call(
        &server,
        &a,
        "fake__emit",
        json!({ "events": [later], "delay": 0.4 }),
    )
    .await;
    call(&server, &b, "fake__emit", json!({})).await;
    let got = notices(&mut events, Duration::from_millis(1500)).await;
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, b.id());
    assert_eq!(got[0].2, "[automatic notice: fake event] dragged a box");

    // Dropped: a session that does not exist, one that has no MCP tools, an event without text.
    let dropped = json!({ "events": [
        { "session": "s-nope", "text": "lost" },
        { "session": single.id(), "text": "lost too" },
        { "session": a.id() },
    ] });
    call(&server, &b, "fake__emit", dropped).await;
    let got = notices(&mut events, Duration::from_millis(1500)).await;
    assert!(got.is_empty(), "{got:?}");

    // A burst: the first at once, the rest together a moment later.
    let fresh = sessions.create(RunKind::Agentic, None, None).unwrap();
    let burst: Vec<Value> = (1..=4)
        .map(|i| json!({ "session": fresh.id(), "text": format!("tick {i}") }))
        .collect();
    let started = std::time::Instant::now();
    call(&server, &b, "fake__emit", json!({ "events": burst })).await;
    let got = notices(&mut events, Duration::from_millis(2500)).await;
    let texts: Vec<&str> = got.iter().map(|(_, _, t)| t.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "[automatic notice: fake event] tick 1",
            "[automatic notice: 3 fake events]\n- tick 2\n- tick 3\n- tick 4",
        ]
    );
    assert!(got.iter().all(|(s, _, _)| s == fresh.id()));
    assert!(started.elapsed() >= Duration::from_millis(900));
}

/// Approves every approval request (here: the site's `mcp.toml`).
fn approve_all(server: &Arc<Server>) {
    let mut events = server.hub.subscribe();
    let server = server.clone();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let ServerMessage::ApprovalRequest { request, .. } = &*event {
                server
                    .pending_approvals
                    .resolve(request, ApprovalDecision::Approve);
            }
        }
    });
}

#[tokio::test]
async fn a_site_server_tells_only_sessions_in_its_site() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let script = scratch.path().join("fake_mcp.py");
    std::fs::write(&script, FAKE_SERVER).unwrap();
    let (dir, server, _provider, _addr) = start_with(Config::defaults(), vec![]).await;
    let lab = dir.path().join("sites/lab");
    std::fs::create_dir_all(&lab).unwrap();
    std::fs::write(lab.join("index.md"), "---\ntitle: lab\n---\n# lab\n").unwrap();
    let toml = format!(
        "[servers.canvas]\ncommand = \"python3\"\nargs = [\"{}\"]\nallow = [\"*\"]\n",
        script.display()
    );
    std::fs::write(lab.join("mcp.toml"), toml).unwrap();
    approve_all(&server);
    let mut events = server.hub.subscribe();
    let inside = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();
    let outside = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();
    let cancel = CancellationToken::new();
    let entered = set_site(&server, &*server.sink(&inside), Some("lab"), &cancel)
        .await
        .unwrap();
    assert!(matches!(entered, SiteSet::Entered(_)), "{entered:?}");

    let both = json!({ "events": [
        { "session": outside.id(), "text": "not yours" },
        { "session": inside.id(), "text": "yours" },
    ] });
    call(&server, &inside, "canvas__emit", both).await;
    let got = notices(&mut events, Duration::from_millis(800)).await;
    assert_eq!(
        got,
        vec![(
            inside.id().to_string(),
            "canvas".to_string(),
            "[automatic notice: canvas event] yours".to_string()
        )]
    );
}

/// A streamable-HTTP server without tools whose GET event stream carries what is sent on the
/// returned channel.
async fn http_server() -> (String, tokio::sync::mpsc::UnboundedSender<Value>) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let receiver = Arc::new(std::sync::Mutex::new(Some(receiver)));
    let app = Router::new().route(
        "/mcp",
        post(|body: String| async move {
            let message: Value = serde_json::from_str(&body).unwrap();
            let Some(id) = message.get("id").cloned() else {
                return axum::http::StatusCode::ACCEPTED.into_response();
            };
            let result = match message["method"].as_str().unwrap() {
                "initialize" => json!({ "protocolVersion": "2025-06-18",
                    "capabilities": { "tools": {} }, "serverInfo": { "name": "h", "version": "1" } }),
                "tools/list" => json!({ "tools": [] }),
                _ => json!({}),
            };
            let mut headers = HeaderMap::new();
            headers.insert("mcp-session-id", "abc".parse().unwrap());
            (
                headers,
                axum::Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })),
            )
                .into_response()
        })
        .get(move |headers: HeaderMap| async move {
            assert_eq!(headers["mcp-session-id"], "abc");
            let Some(receiver) = receiver.lock().unwrap().take() else {
                return axum::http::StatusCode::METHOD_NOT_ALLOWED.into_response();
            };
            let stream = futures::stream::unfold(receiver, |mut receiver| async move {
                let params = receiver.recv().await?;
                let message = json!({ "jsonrpc": "2.0",
                    "method": "notifications/composer/event", "params": params });
                Some((
                    Ok::<_, std::io::Error>(format!("data: {message}\n\n")),
                    receiver,
                ))
            });
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from_stream(stream))
                .unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{addr}/mcp"), sender)
}

#[tokio::test]
async fn an_http_server_sends_events_on_its_event_stream() {
    let (url, sender) = http_server().await;
    let mut config = Config::defaults();
    config.mcp.insert(
        "web".into(),
        McpServerConfig {
            url,
            enabled: true,
            ..Default::default()
        },
    );
    let (_dir, server, _provider, _addr) = start_with(config, vec![]).await;
    let mut events = server.hub.subscribe();
    let session = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();
    // Connecting (listing it) opens its event stream.
    server.mcp().list(&[]).await;
    sender
        .send(json!({ "session": session.id(), "text": "from http" }))
        .unwrap();
    let got = notices(&mut events, Duration::from_millis(1500)).await;
    assert_eq!(
        got,
        vec![(
            session.id().to_string(),
            "web".to_string(),
            "[automatic notice: web event] from http".to_string()
        )]
    );
}
