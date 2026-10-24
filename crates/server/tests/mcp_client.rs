//! MCP servers from `config.toml`: listed with their state (`mcp.list`), their tools offered to the
//! agents that list them, calls approved unless `allow` or an earlier `always` covers them, a
//! server's changed tools picked up; over stdio (a fake server in Python) and streamable HTTP.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use composer_protocol::{ApprovalDecision, ChangedBy, ClientCommand, CommandResult, ServerMessage};
use composer_server::provider::ModelEvent;
use composer_server::{Config, McpServerConfig};
use serde_json::{Value, json};
use support::*;

/// Tools `echo`, `add.numbers` and `boom`; after the first `echo` it also has `late` (and says so).
const FAKE_SERVER: &str = r#"
import json, sys
late = False
def tools():
    t = [
        {"name": "echo", "description": "Echoes text", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}},
        {"name": "add.numbers", "description": "Adds a and b", "inputSchema": {"type": "object"}},
        {"name": "boom", "description": "Always fails"},
    ]
    if late:
        t.append({"name": "late", "description": "Arrived later", "inputSchema": {"type": "object"}})
    return t
def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()
for line in sys.stdin:
    m = json.loads(line)
    method, mid, p = m.get("method"), m.get("id"), m.get("params") or {}
    if mid is None:
        continue
    if method == "initialize":
        r = {"protocolVersion": p["protocolVersion"], "capabilities": {"tools": {"listChanged": True}},
             "serverInfo": {"name": "fake", "version": "1"}}
    elif method == "tools/list":
        r = {"tools": tools()}
    elif method == "tools/call":
        name, args = p["name"], p.get("arguments") or {}
        if name == "echo":
            r = {"content": [{"type": "text", "text": args.get("text", "")}]}
            if not late:
                late = True
                send({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"})
        elif name == "add.numbers":
            r = {"content": [], "structuredContent": {"sum": args["a"] + args["b"]}}
        else:
            r = {"content": [{"type": "text", "text": "it broke"}], "isError": True}
    else:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "no " + method}})
        continue
    send({"jsonrpc": "2.0", "id": mid, "result": r})
"#;

fn has_python() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

async fn command(socket: &mut Socket, id: u64, command: ClientCommand) -> CommandResult {
    send(socket, id, command).await;
    let reply = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    match reply.last() {
        Some(ServerMessage::Ok { result, .. }) => result.clone(),
        other => panic!("{other:?}"),
    }
}

async fn session(socket: &mut Socket, agent: &str) -> String {
    let CommandResult::Session { session } = command(
        socket,
        1,
        ClientCommand::SessionCreate {
            agent: agent.into(),
            project: None,
        },
    )
    .await
    else {
        panic!()
    };
    command(
        socket,
        2,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;
    session.id
}

async fn say(socket: &mut Socket, session: &str, id: u64) {
    send(
        socket,
        id,
        ClientCommand::MessageSend {
            session: session.into(),
            text: "go".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
}

/// A turn that calls one tool, then answers.
fn turn(name: &str, args: Value) -> Vec<Vec<ModelEvent>> {
    vec![
        vec![finished(vec![tool_call("c", name, args)])],
        vec![ModelEvent::Content("done".into()), finished(vec![])],
    ]
}

fn results(events: &[ServerMessage]) -> Vec<(String, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result.clone(), *is_error)),
            _ => None,
        })
        .collect()
}

fn asked(events: &[ServerMessage]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, ServerMessage::ApprovalRequest { .. }))
}

/// Through the server, so the index has it right away.
fn write_agent(server: &composer_server::Server, name: &str, tools: &str) {
    let url = composer_core::SiteUrl::new("agents", &format!("{name}.md")).unwrap();
    let page = format!("---\ntitle: {name}\ntype: agent\ntools: {tools}\n---\nYou use tools.\n");
    server.write_page(&url, &page, ChangedBy::App).unwrap();
}

#[tokio::test]
async fn stdio_servers() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let script = scratch.path().join("fake_mcp.py");
    std::fs::write(&script, FAKE_SERVER).unwrap();
    let mut config = Config::defaults();
    config.mcp.insert(
        "fake".into(),
        McpServerConfig {
            command: "python3".into(),
            args: vec![script.display().to_string()],
            allow: vec!["echo".into()],
            enabled: true,
            ..Default::default()
        },
    );
    config.mcp.insert(
        "broken".into(),
        McpServerConfig {
            command: "/nonexistent/mcp-server".into(),
            enabled: true,
            ..Default::default()
        },
    );
    config.mcp.insert(
        "dormant".into(),
        McpServerConfig {
            url: "http://127.0.0.1:9/mcp".into(),
            enabled: false,
            ..Default::default()
        },
    );

    let mut scripts = turn("fake__echo", json!({ "text": "hi" }));
    scripts.extend(turn("fake__add_numbers", json!({ "a": 2, "b": 3 })));
    scripts.extend(turn("fake__add_numbers", json!({ "a": 1, "b": 1 })));
    scripts.extend(turn("fake__boom", json!({})));
    scripts.extend(turn("fake__echo", json!({ "text": "x" })));
    let (dir, server, provider, addr) = start_with(config, scripts).await;
    write_agent(&server, "mcpuser", "[open, fake__*]");
    write_agent(&server, "picky", "[open, fake__echo]");
    let mut socket = connect(addr, &server.token).await;

    // The servers and their state; listing connects the ones that are on.
    let CommandResult::McpServers { servers } = command(&mut socket, 20, ClientCommand::McpList).await
    else {
        panic!()
    };
    let by_name = |name: &str| servers.iter().find(|s| s.name == name).unwrap().clone();
    let fake = by_name("fake");
    assert_eq!((fake.state.as_str(), fake.transport.as_str()), ("ready", "stdio"));
    assert_eq!(fake.tools, ["fake__echo", "fake__add_numbers", "fake__boom"]);
    assert_eq!(fake.agents, ["mcpuser", "picky"]);
    let broken = by_name("broken");
    assert_eq!(broken.state, "failed");
    assert!(broken.error.unwrap().contains("/nonexistent/mcp-server"));
    let dormant = by_name("dormant");
    assert_eq!((dormant.state.as_str(), dormant.transport.as_str()), ("off", "http"));

    // `allow`ed: runs without asking. The model saw the server's tools, named for it.
    let id = session(&mut socket, "mcpuser").await;
    say(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(!asked(&events));
    assert_eq!(results(&events), [("hi".to_string(), false)]);
    let offered: Vec<String> = provider.requests()[0]
        .tools
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert!(offered.contains(&"open".to_string()));
    assert!(offered.contains(&"fake__add_numbers".to_string()));
    let echo = provider.requests()[0]
        .tools
        .iter()
        .find(|t| t.name == "fake__echo")
        .unwrap()
        .clone();
    assert_eq!(echo.description, "[fake] Echoes text");

    // Not allowed: asks; `always` remembers it, so the next call doesn't ask.
    say(&mut socket, &id, 4).await;
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest {
        request, title, body, ..
    }) = events.last()
    else {
        panic!()
    };
    assert_eq!(title, "MCP fake: add.numbers");
    assert!(body.contains("\"a\": 2"), "{body}");
    send(
        &mut socket,
        5,
        ClientCommand::ApprovalRespond {
            request: request.clone(),
            decision: ApprovalDecision::Always,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert_eq!(results(&events), [("{\n  \"sum\": 5\n}".to_string(), false)]);
    say(&mut socket, &id, 6).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(!asked(&events));
    assert_eq!(results(&events)[0].0, "{\n  \"sum\": 2\n}");
    let rules = std::fs::read_to_string(dir.path().join("state/tool_rules.json")).unwrap();
    assert!(rules.contains("\"mcp:fake\"") && rules.contains("fake__add_numbers"), "{rules}");

    // A failed call is a result the model sees; the server said its tools changed after the
    // first echo, so the next turn offers `late` too.
    say(&mut socket, &id, 7).await;
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest { request, .. }) = events.last() else {
        panic!()
    };
    send(
        &mut socket,
        8,
        ClientCommand::ApprovalRespond {
            request: request.clone(),
            decision: ApprovalDecision::Approve,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert_eq!(results(&events), [("it broke".to_string(), true)]);
    let last = provider.requests().last().unwrap().clone();
    assert!(last.tools.iter().any(|t| t.name == "fake__late"));

    // An agent listing one tool gets that one; calling another is unknown to it.
    let picky = session(&mut socket, "picky").await;
    say(&mut socket, &picky, 9).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert_eq!(results(&events), [("x".to_string(), false)]);
    let mcp: Vec<String> = provider.requests().last().unwrap()
        .tools
        .iter()
        .filter(|t| t.name.contains("__"))
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(mcp, ["fake__echo"]);
}

/// A streamable-HTTP server: a session id from `initialize`, required afterwards; `tools/list`
/// answered as an event stream, calls as JSON.
async fn fake_http(headers: HeaderMap, body: String) -> Response {
    let m: Value = serde_json::from_str(&body).unwrap();
    let method = m["method"].as_str().unwrap_or_default();
    let id = m.get("id").cloned();
    if method != "initialize" {
        let session = headers.get("mcp-session-id").and_then(|v| v.to_str().ok());
        let version = headers.get("mcp-protocol-version").and_then(|v| v.to_str().ok());
        if session != Some("s-1") || version != Some("2025-06-18") {
            return (StatusCode::BAD_REQUEST, "no session").into_response();
        }
    }
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer tok-123") {
        return (StatusCode::UNAUTHORIZED, "who are you").into_response();
    }
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };
    let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string();
    match method {
        "initialize" => (
            [("mcp-session-id", "s-1"), ("content-type", "application/json")],
            reply(json!({ "protocolVersion": "2025-06-18", "capabilities": { "tools": {} },
                "serverInfo": { "name": "web", "version": "1" } })),
        )
            .into_response(),
        "tools/list" => {
            let note = json!({ "jsonrpc": "2.0", "method": "notifications/message",
                "params": { "level": "info", "data": "listing" } });
            let stream = format!(
                "event: message\ndata: {note}\n\nevent: message\ndata: {}\n\n",
                reply(json!({ "tools": [{ "name": "lookup", "description": "Looks up",
                    "inputSchema": { "type": "object" } }] }))
            );
            ([("content-type", "text/event-stream")], stream).into_response()
        }
        "tools/call" => (
            [("content-type", "application/json")],
            reply(json!({ "content": [{ "type": "text",
                "text": format!("found {}", m["params"]["arguments"]["q"]) }] })),
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::test]
async fn http_servers() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let web = listener.local_addr().unwrap();
    let served = Arc::new(AtomicBool::new(false));
    let flag = served.clone();
    let app = Router::new().route(
        "/mcp",
        post(move |headers: HeaderMap, body: String| {
            flag.store(true, Ordering::SeqCst);
            fake_http(headers, body)
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    // SAFETY: only this test reads the variable.
    unsafe { std::env::set_var("COMPOSER_TEST_WEB_TOKEN", "tok-123") };
    let mut config = Config::defaults();
    config.mcp.insert(
        "web".into(),
        McpServerConfig {
            url: format!("http://{web}/mcp"),
            headers: [(
                "Authorization".to_string(),
                "Bearer ${COMPOSER_TEST_WEB_TOKEN}".to_string(),
            )]
            .into(),
            allow: vec!["*".into()],
            enabled: true,
            ..Default::default()
        },
    );
    let (_dir, server, provider, addr) =
        start_with(config, turn("web__lookup", json!({ "q": "rust" }))).await;
    write_agent(&server, "searcher", "[web__*]");
    let mut socket = connect(addr, &server.token).await;

    let id = session(&mut socket, "searcher").await;
    say(&mut socket, &id, 2).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(served.load(Ordering::SeqCst));
    assert!(!asked(&events));
    assert_eq!(results(&events), [("found \"rust\"".to_string(), false)]);
    let offered: Vec<String> = provider.requests()[0]
        .tools
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(offered, ["web__lookup"]);
}
