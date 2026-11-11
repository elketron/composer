//! Server-owned terminals (ticket 0176): a session's terminal is a shell in the workspace's
//! sandbox, in its project, a tab on its desk; its output streams to the session's clients and a
//! late one gets the scrollback; the agent reads it, and types into it only when the user
//! approves; an idle one is reaped and opens again fresh where it was.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use composer_protocol::{ApprovalDecision, ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use composer_server::agent::AgentDefinition;
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn has(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The terminal output seen on `socket` until it contains `wanted` (10 s).
async fn output_until(socket: &mut Socket, id: &str, wanted: &str) -> String {
    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !seen.contains(wanted) {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(event) = tokio::time::timeout(left, next_event(socket)).await else {
            panic!("no `{wanted}` in: {seen}")
        };
        if let ServerMessage::TerminalOutput {
            terminal: of, data, ..
        } = event
            && of == id
        {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap();
            seen.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    seen
}

async fn reply(socket: &mut Socket, id: u64, command: ClientCommand) -> CommandResult {
    send(socket, id, command).await;
    let events = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    match events.last() {
        Some(ServerMessage::Ok { result, .. }) => result.clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_sessions_terminal_runs_in_its_sandbox() {
    if !has("bwrap") {
        return;
    }
    // SAFETY: before the server starts; the only test in this binary.
    unsafe { std::env::set_var("SHELL", "/bin/sh") };
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    let game = root.join("game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join("README"), "the game\n").unwrap();
    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let session = server
        .sessions
        .create("default", Some(game.display().to_string()))
        .unwrap();
    let sid = session.id().to_string();
    let mut client = connect(addr, &server.token).await;
    reply(
        &mut client,
        1,
        ClientCommand::SessionAttach {
            session: sid.clone(),
            since: None,
        },
    )
    .await;

    let CommandResult::Terminal { terminal: id, .. } = reply(
        &mut client,
        2,
        ClientCommand::TerminalOpen {
            session: sid.clone(),
            terminal: None,
            cols: 100,
            rows: 30,
        },
    )
    .await
    else {
        panic!()
    };
    assert_eq!(session.desk().active, Some(format!("terminal:{id}")));
    // In its project, in the sandbox: no state/ (the workspace's token).
    let token = dir.path().join("state/token");
    assert!(token.exists());
    let input = |data: String| ClientCommand::TerminalInput {
        terminal: id.clone(),
        data,
    };
    reply(
        &mut client,
        3,
        input(format!(
            "echo at-$(pwd) sum-$((40+2)); cat README; test -e {} && echo $((0+1))-LEAK || echo $((1+1))-SAFE\r",
            token.display()
        )),
    )
    .await;
    let seen = output_until(&mut client, &id, "2-SAFE").await;
    assert!(seen.contains(&format!("at-{}", game.display())), "{seen}");
    assert!(
        seen.contains("sum-42") && seen.contains("the game") && !seen.contains("1-LEAK"),
        "{seen}"
    );
    reply(
        &mut client,
        4,
        ClientCommand::TerminalResize {
            terminal: id.clone(),
            cols: 120,
            rows: 40,
        },
    )
    .await;

    // A late client gets the scrollback; the agent reads it.
    let mut late = connect(addr, &server.token).await;
    let CommandResult::Terminal { scrollback, .. } = reply(
        &mut late,
        5,
        ClientCommand::TerminalRead {
            terminal: id.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(scrollback)
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("sum-42"));
    let context = ToolContext {
        server: server.clone(),
        session: session.clone(),
        agent: Arc::new(AgentDefinition::load(&server.web, "default").unwrap()),
        site: None,
        cancel: CancellationToken::new(),
    };
    let read = server
        .tools
        .call_value(&context, "terminal_read", json!({ "id": id }))
        .await;
    assert!(read.text.contains("sum-42"), "{}", read.text);

    // Another session's agent can't read or type into it.
    let other = server.sessions.create("default", None).unwrap();
    let elsewhere = ToolContext {
        session: other,
        ..context.clone()
    };
    for (tool, args) in [
        ("terminal_read", json!({ "id": id })),
        ("terminal_send", json!({ "id": id, "text": "echo nope" })),
    ] {
        let refused = server.tools.call_value(&elsewhere, tool, args).await;
        assert!(
            refused.is_error && refused.text.contains("isn't this session's"),
            "{tool}: {}",
            refused.text
        );
    }

    // Typing asks the user: denied, nothing typed; approved, typed.
    for (decision, typed) in [
        (ApprovalDecision::Deny, false),
        (ApprovalDecision::Approve, true),
    ] {
        let (server2, context2, id2) = (server.clone(), context.clone(), id.clone());
        let call = tokio::spawn(async move {
            server2
                .tools
                .call_value(
                    &context2,
                    "terminal_send",
                    json!({ "id": id2, "text": "echo typed-by-the-agent" }),
                )
                .await
        });
        let request = loop {
            let event = tokio::time::timeout(Duration::from_secs(10), next_event(&mut client))
                .await
                .expect("an approval request");
            if let ServerMessage::ApprovalRequest { request, title, .. } = event {
                assert!(title.contains("Type into terminal"), "{title}");
                break request;
            }
        };
        reply(
            &mut client,
            6,
            ClientCommand::ApprovalRespond { request, decision },
        )
        .await;
        let result = call.await.unwrap();
        assert_eq!(result.is_error, !typed, "{}", result.text);
    }
    output_until(&mut client, &id, "typed-by-the-agent\r\n").await;

    // Idle: reaped; opened again by its id, a fresh shell where it was.
    let reaped = server.terminals.reap(&server, Duration::ZERO);
    assert_eq!(reaped, std::slice::from_ref(&id));
    reply(
        &mut client,
        7,
        ClientCommand::TerminalOpen {
            session: sid.clone(),
            terminal: Some(id.clone()),
            cols: 80,
            rows: 24,
        },
    )
    .await;
    reply(&mut client, 8, input("pwd\r".into())).await;
    output_until(&mut client, &id, &game.display().to_string()).await;
    reply(
        &mut client,
        9,
        ClientCommand::TerminalClose {
            terminal: id.clone(),
        },
    )
    .await;
    assert!(server.terminals.get(&id).is_none());
}
