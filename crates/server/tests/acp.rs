//! A session on an outside agent over the Agent Client Protocol, against a fake agent
//! (`support/fake_acp_agent.py`): composer starts it, hands it itself as its MCP server, turns its
//! updates into the session's events, its permission requests into approvals, an interrupt into
//! `session/cancel`, and ends it with the session.

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use composer_protocol::http::HookReply;
use composer_protocol::{
    ApprovalDecision, ApprovalKind, ClientCommand, CommandResult, RunKind, ServerMessage,
    SessionInfo,
};
use composer_server::Config;
use composer_server::harness::AcpConfig;
use composer_server::provider::ModelEvent;
use composer_server::runs::HookConfig;
use serde_json::{Value, json};
use support::*;

/// A config with the fake agent as `[acp.fake]`, logging what it receives to `log`.
fn config(log: &Path) -> Config {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp_agent.py");
    let mut config = Config::defaults();
    config.acp.insert(
        "fake".into(),
        AcpConfig {
            command: "python3".into(),
            args: vec![script.display().to_string()],
            env: [("FAKE_ACP_LOG".to_string(), log.display().to_string())].into(),
        },
    );
    config
}

/// What the fake agent received so far (and its pid, first).
fn received(log: &Path) -> Vec<Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Waits until the agent has received a message that `found` matches; returns it.
async fn wait_received(log: &Path, found: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..200 {
        if let Some(message) = received(log).into_iter().find(&found) {
            return message;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the agent never got it; it has {:#?}", received(log));
}

fn method(name: &'static str) -> impl Fn(&Value) -> bool {
    move |m| m["method"] == name
}

async fn create(socket: &mut Socket, kind: RunKind, harness: &str) -> ServerMessage {
    send(
        socket,
        1,
        ClientCommand::SessionCreate {
            kind,
            model: None,
            site: None,
            harness: Some(harness.into()),
        },
    )
    .await;
    until(socket, |e| {
        matches!(
            e,
            ServerMessage::Ok { id: 1, .. } | ServerMessage::Error { id: Some(1), .. }
        )
    })
    .await
    .pop()
    .unwrap()
}

/// A session on the fake agent, attached.
async fn session(socket: &mut Socket) -> SessionInfo {
    let ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    } = create(socket, RunKind::Agentic, "fake").await
    else {
        panic!("not created")
    };
    send(
        socket,
        2,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;
    until(socket, |e| matches!(e, ServerMessage::Ok { id: 2, .. })).await;
    session
}

async fn say(socket: &mut Socket, session: &str, text: &str) {
    send(
        socket,
        3,
        ClientCommand::MessageSend {
            session: session.into(),
            text: text.into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
}

fn alive(pid: u64) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

#[tokio::test]
async fn the_agent_gets_composer_as_its_mcp_server_and_its_turn_lands_in_the_transcript() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let (_dir, server, provider, addr) = start_with(config(&log), vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    let session = session(&mut socket).await;
    assert_eq!(session.harness.as_deref(), Some("fake"));

    // Started with the session: initialize, then session/new with composer as its MCP server.
    let initialize = wait_received(&log, method("initialize")).await;
    assert_eq!(initialize["params"]["protocolVersion"], 1);
    assert_eq!(initialize["params"]["clientInfo"]["name"], "composer");
    let new = wait_received(&log, method("session/new")).await;
    assert_eq!(
        Path::new(new["params"]["cwd"].as_str().unwrap()),
        server.home.sites()
    );
    let servers = new["params"]["mcpServers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["name"], "composer");
    assert_eq!(
        servers[0]["command"].as_str().unwrap(),
        std::env::current_exe().unwrap().display().to_string()
    );
    assert_eq!(
        servers[0]["args"],
        json!([
            "mcp",
            "--session",
            session.id,
            "--server",
            format!("http://{addr}")
        ])
    );
    assert_eq!(
        servers[0]["env"],
        json!([{ "name": "COMPOSER_HOME", "value": server.home.root().display().to_string() }])
    );

    say(&mut socket, &session.id, "hello").await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let prompt = wait_received(&log, method("session/prompt")).await;
    assert_eq!(prompt["params"]["sessionId"], "fake-session");
    assert_eq!(
        prompt["params"]["prompt"],
        json!([{ "type": "text", "text": "hello" }])
    );

    let shown: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::ThinkingDelta { text, .. } => Some(format!("thinking {text}")),
            ServerMessage::MessageDelta { text, .. } => Some(format!("delta {text}")),
            ServerMessage::MessageDone { message, .. } => Some(format!(
                "{:?} {} [{}]",
                message.role,
                message.content,
                message
                    .tool_calls
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )),
            ServerMessage::ToolCallStarted { name, args, .. } => {
                Some(format!("tool {name} {args}"))
            }
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some(format!("result {result} {is_error}")),
            ServerMessage::TurnDone { cancelled, .. } => Some(format!("done {cancelled}")),
            _ => None,
        })
        .collect();
    assert_eq!(
        shown,
        [
            "User hello []",
            "thinking thinking it over",
            "delta Hello ",
            "Assistant Hello  [Read notes.md]",
            r#"tool Read notes.md {"path":"notes.md"}"#,
            "result the notes false",
            "delta world",
            "Assistant world []",
            "done false",
        ]
    );
    assert!(provider.requests().is_empty(), "composer's own model ran");

    // In the transcript: a client attaching later sees the conversation.
    let mut other = connect(addr, &server.token).await;
    send(
        &mut other,
        1,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;
    let replayed = until(&mut other, |e| matches!(e, ServerMessage::Ok { id: 1, .. })).await;
    assert!(replayed.iter().any(|e| matches!(
        e,
        ServerMessage::MessageDone { message, .. } if message.content == "world"
    )));
}

#[tokio::test]
async fn a_permission_request_is_an_approval_and_the_answer_goes_back() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let (_dir, server, _provider, addr) = start_with(config(&log), vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    let session = session(&mut socket).await;

    say(&mut socket, &session.id, "permission please").await;
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest {
        request,
        kind,
        title,
        body,
        rule,
        ..
    }) = events.last()
    else {
        unreachable!()
    };
    assert_eq!(*kind, ApprovalKind::Tool);
    assert_eq!(title, "Run rm -rf build");
    assert!(body.contains("rm -rf build"), "{body}");
    assert_eq!(rule.as_deref(), Some("Always allow rm"));
    send(
        &mut socket,
        9,
        ClientCommand::ApprovalRespond {
            request: request.clone(),
            decision: ApprovalDecision::Always,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(events.iter().any(|e| matches!(
        e,
        ServerMessage::MessageDone { message, .. } if message.content == "you chose always"
    )));
    let answer = wait_received(&log, |m| {
        m["id"].is_number() && m["result"]["outcome"].is_object()
    })
    .await;
    assert_eq!(
        answer["result"]["outcome"],
        json!({ "outcome": "selected", "optionId": "always" })
    );
}

#[tokio::test]
async fn an_interrupt_is_session_cancel() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let (_dir, server, _provider, addr) = start_with(config(&log), vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    let session = session(&mut socket).await;

    say(&mut socket, &session.id, "wait for me").await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::MessageDelta { .. })
    })
    .await;
    send(
        &mut socket,
        4,
        ClientCommand::TurnCancel {
            session: session.id.clone(),
            seen: None,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(matches!(
        events.last(),
        Some(ServerMessage::TurnDone {
            cancelled: true,
            ..
        })
    ));
    let cancel = wait_received(&log, method("session/cancel")).await;
    assert_eq!(cancel["params"]["sessionId"], "fake-session");
    assert!(cancel.get("id").is_none(), "a notification");
}

#[tokio::test]
async fn only_an_agentic_session_may_have_a_harness() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let (_dir, server, _provider, addr) = start_with(config(&log), vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    let ServerMessage::Error { message, .. } = create(&mut socket, RunKind::Workflow, "fake").await
    else {
        panic!("a workflow session on an outside agent was created")
    };
    assert_eq!(
        message,
        "an outside agent (harness `fake`) runs agentic sessions only, not a workflow run"
    );
    let ServerMessage::Error { message, .. } = create(&mut socket, RunKind::Agentic, "nope").await
    else {
        panic!("a session on an unknown harness was created")
    };
    assert_eq!(message, "no harness `nope`; config.toml has fake");
    assert!(server.sessions.list().is_empty());
    assert!(!log.exists(), "no agent started");
}

#[tokio::test]
async fn the_agent_ends_with_its_session() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let (_dir, server, _provider, addr) = start_with(config(&log), vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    let session = session(&mut socket).await;
    let pid = wait_received(&log, |m| m["pid"].is_number()).await["pid"]
        .as_u64()
        .unwrap();
    wait_received(&log, method("session/new")).await;
    assert!(alive(pid));

    send(
        &mut socket,
        5,
        ClientCommand::SessionDelete {
            session: session.id.clone(),
        },
    )
    .await;
    until(
        &mut socket,
        |e| matches!(e, ServerMessage::SessionDeleted { session: id } if *id == session.id),
    )
    .await;
    assert!(server.sessions.get(&session.id).is_err());
    for _ in 0..100 {
        if !alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the agent ({pid}) outlived its session");
}

#[tokio::test]
async fn a_stored_hook_run_on_an_outside_agent_answers_with_its_last_text() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let mut config = config(&log);
    config.hooks.insert(
        "ask".into(),
        HookConfig {
            token_file: String::new(),
            token: "hook-secret".into(),
            kind: RunKind::Agentic,
            site: None,
            folder: None,
            prompt: String::new(),
            harness: Some("fake".into()),
            store: true,
            timeout_secs: 30,
        },
    );
    let (_dir, server, _provider, addr) = start_with(config, vec![]).await;
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/api/hooks/ask"))
        .bearer_auth("hook-secret")
        .json(&json!("hello"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let reply: HookReply = response.json().await.unwrap();
    assert_eq!(reply.value, json!("world"));
    let id = reply.session.unwrap();
    assert_eq!(
        server.sessions.get(&id).unwrap().info().harness.as_deref(),
        Some("fake")
    );
    // Stopping the server ends its agents.
    let pid = wait_received(&log, |m| m["pid"].is_number()).await["pid"]
        .as_u64()
        .unwrap();
    server.acp.stop_all().await;
    assert!(!alive(pid), "the agent ({pid}) outlived the server");
}

#[tokio::test]
async fn spawn_run_starts_an_agentic_run_on_an_outside_agent_and_refuses_other_kinds() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let (_dir, server, _provider, addr) = start_with(
        config(&log),
        vec![
            vec![finished(vec![tool_call(
                "c1",
                "spawn_run",
                json!({ "kind": "workflow", "prompt": "count", "harness": "fake" }),
            )])],
            vec![finished(vec![tool_call(
                "c2",
                "spawn_run",
                json!({ "kind": "agentic", "prompt": "hello", "harness": "fake", "wait": true }),
            )])],
            vec![ModelEvent::Content("ok".into()), finished(vec![])],
        ],
    )
    .await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            kind: RunKind::Agentic,
            model: None,
            site: None,
            harness: None,
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;
    say(&mut socket, &session.id, "go").await;
    let events = until(
        &mut socket,
        |e| matches!(e, ServerMessage::TurnDone { session: id, .. } if *id == session.id),
    )
    .await;
    let results: Vec<(String, bool)> = events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result.clone(), *is_error)),
            _ => None,
        })
        .collect();
    assert_eq!(
        results[0],
        (
            "an outside agent (harness `fake`) runs agentic sessions only, not a workflow run"
                .to_string(),
            true
        )
    );
    let child = server
        .sessions
        .list()
        .into_iter()
        .find(|s| s.parent.as_deref() == Some(session.id.as_str()))
        .unwrap();
    assert_eq!(child.harness.as_deref(), Some("fake"));
    assert_eq!(
        results[1],
        (format!("session {} answered:\nworld", child.id), false)
    );
    server.acp.stop_all().await;
}
