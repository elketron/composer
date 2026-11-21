//! MCP servers from `config.toml`: listed with their state (`mcp.list`), their tools offered to
//! every session, calls approved unless `allow` or an earlier `always` covers them, a
//! server's changed tools picked up; over stdio (a fake server in Python) and streamable HTTP.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use composer_protocol::{
    ApprovalDecision, ChangedBy, ClientCommand, CommandResult, RunKind, ServerMessage,
};
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

/// A new session, in `site`.
async fn session(socket: &mut Socket, site: &str) -> String {
    let CommandResult::Session { session } = command(
        socket,
        1,
        ClientCommand::SessionCreate {
            kind: RunKind::Agentic,
            model: None,
            site: None,
        },
    )
    .await
    else {
        panic!()
    };
    command(
        socket,
        90,
        ClientCommand::SiteSet {
            session: session.id.clone(),
            site: Some(site.into()),
        },
    )
    .await;
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

/// A site; through the server, so the index has it right away.
fn write_site(server: &composer_server::Server, name: &str) {
    std::fs::create_dir_all(server.home.sites().join(name)).unwrap();
    let url = composer_core::SiteUrl::new(name, "index.md").unwrap();
    let page = format!("---\ntitle: {name}\n---\nYou use tools.\n");
    server.write_page(&url, &page, ChangedBy::External).unwrap();
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
    let (dir, server, provider, addr) = start_with(config, scripts).await;
    write_site(&server, "tools");
    let mut socket = connect(addr, &server.token).await;

    // The servers and their state; listing connects the ones that are on.
    let CommandResult::McpServers { servers } =
        command(&mut socket, 20, ClientCommand::McpList).await
    else {
        panic!()
    };
    let by_name = |name: &str| servers.iter().find(|s| s.name == name).unwrap().clone();
    let fake = by_name("fake");
    assert_eq!(
        (fake.state.as_str(), fake.transport.as_str()),
        ("ready", "stdio")
    );
    assert_eq!(
        fake.tools,
        ["fake__echo", "fake__add_numbers", "fake__boom"]
    );
    assert!(
        fake.sites.is_empty(),
        "a config.toml server is in every session"
    );
    let broken = by_name("broken");
    assert_eq!(broken.state, "failed");
    assert!(broken.error.unwrap().contains("/nonexistent/mcp-server"));
    let dormant = by_name("dormant");
    assert_eq!(
        (dormant.state.as_str(), dormant.transport.as_str()),
        ("off", "http")
    );

    // `allow`ed: runs without asking. The model saw the server's tools, named for it.
    let id = session(&mut socket, "tools").await;
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
        request,
        title,
        body,
        ..
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
    assert_eq!(
        results(&events),
        [("{\n  \"sum\": 5\n}".to_string(), false)]
    );
    say(&mut socket, &id, 6).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(!asked(&events));
    assert_eq!(results(&events)[0].0, "{\n  \"sum\": 2\n}");
    let rules = std::fs::read_to_string(dir.path().join("state/tool_rules.json")).unwrap();
    assert!(
        rules.contains("\"mcp:fake\"") && rules.contains("fake__add_numbers"),
        "{rules}"
    );

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
}

/// A 2x2 red PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAE0lEQVR4nGP4z8DwnwGM/zMwAAAf7gP9NRsAMwAAAABJRU5ErkJggg==";

/// A streamable-HTTP server: a session id from `initialize`, required afterwards; `tools/list`
/// answered as an event stream, calls as JSON.
async fn fake_http(headers: HeaderMap, body: String) -> Response {
    let m: Value = serde_json::from_str(&body).unwrap();
    let method = m["method"].as_str().unwrap_or_default();
    let id = m.get("id").cloned();
    if method != "initialize" {
        let session = headers.get("mcp-session-id").and_then(|v| v.to_str().ok());
        let version = headers
            .get("mcp-protocol-version")
            .and_then(|v| v.to_str().ok());
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
            [
                ("mcp-session-id", "s-1"),
                ("content-type", "application/json"),
            ],
            reply(
                json!({ "protocolVersion": "2025-06-18", "capabilities": { "tools": {} },
                "serverInfo": { "name": "web", "version": "1" } }),
            ),
        )
            .into_response(),
        "tools/list" => {
            let note = json!({ "jsonrpc": "2.0", "method": "notifications/message",
                "params": { "level": "info", "data": "listing" } });
            let stream = format!(
                "event: message\ndata: {note}\n\nevent: message\ndata: {}\n\n",
                reply(
                    json!({ "tools": [{ "name": "lookup", "description": "Looks up",
                    "inputSchema": { "type": "object" } }] })
                )
            );
            ([("content-type", "text/event-stream")], stream).into_response()
        }
        // `q: screen`: a screenshot beside the text, as a UI-driving server sends one.
        "tools/call" if m["params"]["arguments"]["q"] == "screen" => (
            [("content-type", "application/json")],
            reply(
                json!({ "content": [{ "type": "text", "text": "the screen" },
                { "type": "image", "mimeType": "image/png", "data": PNG }] }),
            ),
        )
            .into_response(),
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
    let mut scripts = turn("web__lookup", json!({ "q": "rust" }));
    scripts.extend(turn("web__lookup", json!({ "q": "screen" })));
    let (_dir, server, provider, addr) = start_with(config, scripts).await;
    write_site(&server, "search");
    let mut socket = connect(addr, &server.token).await;

    let id = session(&mut socket, "search").await;
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
    assert!(offered.contains(&"web__lookup".to_string()), "{offered:?}");

    // A screenshot goes to the model as an image in the tool message (history keeps a stored
    // attachment; the request a data URL).
    say(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert_eq!(
        results(&events),
        [("the screen\n[image (image/png)]".to_string(), false)]
    );
    let after = &provider.requests()[3];
    let images: Vec<&String> = after
        .messages
        .iter()
        .filter_map(|m| match m {
            composer_server::provider::ModelMessage::Tool { images, .. } => Some(images),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(images.len(), 1, "{:?}", after.messages);
    assert!(images[0].starts_with("data:image/png;base64,"));
    let stored = std::fs::read_to_string(
        _dir.path()
            .join(format!("state/sessions/{id}/history.jsonl")),
    )
    .unwrap();
    assert!(stored.contains("attachment://"), "{stored}");
    assert!(!stored.contains(PNG));
}

/// `bearer_file` (what wayai asks for): the file's token is the bearer header of every request,
/// read at each connect; no `Origin` goes along; without the setting there is no header; a file
/// that cannot be read fails the connect with its path; an `Authorization` in `headers` wins.
#[tokio::test]
async fn an_http_server_gets_the_token_of_its_bearer_file() {
    type Seen = Arc<std::sync::Mutex<Vec<(String, Option<String>, Option<String>)>>>;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let web = listener.local_addr().unwrap();
    let seen = Seen::default();
    let record = seen.clone();
    let app = Router::new().route(
        "/{name}",
        post(
            move |axum::extract::Path(name): axum::extract::Path<String>,
                  headers: HeaderMap,
                  body: String| {
                let get = |header: &str| {
                    headers
                        .get(header)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string)
                };
                record
                    .lock()
                    .unwrap()
                    .push((name, get("authorization"), get("origin")));
                fake_http(headers, body)
            },
        ),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let files = tempfile::tempdir().unwrap();
    let file = |name: &str, content: Option<&str>| {
        let path = files.path().join(name);
        if let Some(content) = content {
            std::fs::write(&path, content).unwrap();
        }
        path.display().to_string()
    };
    let entry = |name: &str, bearer_file: String, authorization: Option<&str>| McpServerConfig {
        url: format!("http://{web}/{name}"),
        bearer_file,
        headers: authorization
            .map(|value| ("authorization".to_string(), value.to_string()))
            .into_iter()
            .collect(),
        enabled: true,
        ..Default::default()
    };
    let mut config = Config::defaults();
    let late = file("late", Some("old-token\n"));
    let missing = file("missing", None);
    for (name, entry) in [
        (
            "with",
            entry("with", file("with", Some("  tok-123\n")), None),
        ),
        ("without", entry("without", String::new(), None)),
        ("missing", entry("missing", missing.clone(), None)),
        (
            "own",
            entry("own", file("own", Some("not-this")), Some("Bearer tok-123")),
        ),
        ("late", entry("late", late.clone(), None)),
    ] {
        config.mcp.insert(name.into(), entry);
    }
    let (_dir, server, _provider, _addr) = start_with(config, vec![]).await;
    let listed = server.mcp().list(&[]).await;
    let state = |name: &str| {
        let info = listed.iter().find(|s| s.name == name).unwrap();
        (info.state.clone(), info.error.clone().unwrap_or_default())
    };
    let requests = |name: &str| -> Vec<(Option<String>, Option<String>)> {
        seen.lock()
            .unwrap()
            .iter()
            .filter(|(to, _, _)| to == name)
            .map(|(_, authorization, origin)| (authorization.clone(), origin.clone()))
            .collect()
    };

    assert_eq!(state("with").0, "ready", "{}", state("with").1);
    let with = requests("with");
    assert!(
        with.len() >= 3,
        "initialize, initialized, tools/list: {with:?}"
    );
    for (authorization, origin) in &with {
        assert_eq!(authorization.as_deref(), Some("Bearer tok-123"));
        assert_eq!(origin, &None, "no Origin goes to the server");
    }

    let (failed, error) = state("without");
    assert_eq!(failed, "failed");
    assert!(error.contains("401"), "{error}");
    assert_eq!(
        requests("without"),
        [(None, None)],
        "no header without the setting"
    );

    let (failed, error) = state("missing");
    assert_eq!(failed, "failed");
    assert!(
        error.contains(&missing) && error.contains("bearer token file"),
        "{error}"
    );
    assert!(requests("missing").is_empty(), "nothing was sent");

    assert_eq!(state("own").0, "ready", "{}", state("own").1);
    assert!(
        requests("own")
            .iter()
            .all(|(authorization, _)| authorization.as_deref() == Some("Bearer tok-123"))
    );

    // The file is read at every connect: a token written later is the one sent.
    assert_eq!(state("late").0, "failed");
    assert_eq!(
        requests("late"),
        [(Some("Bearer old-token".to_string()), None)]
    );
    std::fs::write(&late, "tok-123").unwrap();
    let listed = server.mcp().list(&[]).await;
    let late = listed.iter().find(|s| s.name == "late").unwrap();
    assert_eq!(late.state, "ready", "{:?}", late.error);
}
