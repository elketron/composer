//! The whole server against a scripted model, driven over the real WebSocket and HTTP API.

mod support;

use composer_protocol::{
    ChangedBy, ClientCommand, CommandResult, PageChangeKind, Role, ServerMessage,
};
use composer_server::provider::{ModelEvent, ModelMessage, Usage};
use serde_json::json;
use support::*;

async fn create_session(socket: &mut Socket) -> String {
    send(
        socket,
        1,
        ClientCommand::SessionCreate {
            agent: "default".into(),
            project: None,
        },
    )
    .await;
    let reply = until(socket, |e| matches!(e, ServerMessage::Ok { id: 1, .. })).await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!("no session: {reply:?}")
    };
    session.id.clone()
}

async fn attach(socket: &mut Socket, session: &str) {
    send(
        socket,
        2,
        ClientCommand::SessionAttach {
            session: session.into(),
            since: None,
        },
    )
    .await;
    until(socket, |e| matches!(e, ServerMessage::Ok { id: 2, .. })).await;
}

#[tokio::test]
async fn turn_with_tool_call() {
    let (_dir, server, provider, addr) = start(vec![
        vec![
            ModelEvent::Thinking("I should read the user page.".into()),
            finished(vec![tool_call(
                "c1",
                "open",
                json!({ "url": "site://memory/user" }),
            )]),
        ],
        vec![
            ModelEvent::Content("Nothing is ".into()),
            ModelEvent::Content("recorded yet.".into()),
            ModelEvent::Finished {
                tool_calls: vec![],
                usage: Some(Usage {
                    prompt_tokens: 1200,
                    completion_tokens: 8,
                }),
                finish_reason: Some("stop".into()),
            },
        ],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let session = create_session(&mut socket).await;
    attach(&mut socket, &session).await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: session.clone(),
            text: "what do you know about me?".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;

    let kinds: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::TurnStarted { .. } => Some("turn.started"),
            ServerMessage::ThinkingDelta { .. } => Some("thinking"),
            ServerMessage::MessageDelta { .. } => Some("delta"),
            ServerMessage::MessageDone { message, .. } if message.role == Role::User => {
                Some("user")
            }
            ServerMessage::MessageDone { message, .. } if message.role == Role::Assistant => {
                Some("assistant")
            }
            ServerMessage::ToolCallStarted { .. } => Some("tool.call"),
            ServerMessage::AgentNavigated { .. } => Some("navigated"),
            ServerMessage::ToolResult { .. } => Some("tool.result"),
            ServerMessage::TurnDone { .. } => Some("turn.done"),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "user",
            "turn.started",
            "thinking",
            "assistant",
            "tool.call",
            "navigated",
            "tool.result",
            "delta",
            "delta",
            "assistant",
            "turn.done"
        ]
    );

    let result = events.iter().find_map(|e| match e {
        ServerMessage::ToolResult {
            result, is_error, ..
        } => Some((result.clone(), *is_error)),
        _ => None,
    });
    let (result, is_error) = result.unwrap();
    assert!(
        !is_error && result.starts_with("url: site://memory/user.md") && result.contains("# User"),
        "{result}"
    );
    assert!(events.iter().any(
        |e| matches!(e, ServerMessage::AgentNavigated { url, .. } if url == "site://memory/user.md")
    ));

    // Second model call saw the system prompt, the tool result, and this turn's thinking.
    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    let ModelMessage::System { content } = &requests[1].messages[0] else {
        panic!("no system prompt")
    };
    assert!(
        content.contains("You are the user's assistant")
            && content.contains("<page url=\"site://home\">")
    );
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|m| matches!(m, ModelMessage::Tool { call_id, .. } if call_id == "c1"))
    );
    assert!(requests[1].messages.iter().any(|m| matches!(
        m,
        ModelMessage::Assistant {
            thinking: Some(_),
            ..
        }
    )));
    assert!(requests[0].tools.iter().any(|t| t.name == "edit"));

    let info = server.sessions.get(&session).unwrap().info();
    assert_eq!(info.context_tokens, 1208);
    assert_eq!(info.preview.as_deref(), Some("what do you know about me?"));
}

#[tokio::test]
async fn write_updates_index_and_notifies_subscribers() {
    let content =
        "---\ntitle: composer\ntype: memory\nkind: project\n---\n# composer\nRust + Ratatui.\n";
    let (_dir, server, _provider, addr) = start(vec![
        vec![finished(vec![tool_call(
            "w1",
            "write",
            json!({ "url": "memory/projects/composer", "content": content }),
        )])],
        vec![ModelEvent::Content("Saved.".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        9,
        ClientCommand::Subscribe {
            urls: vec!["site://memory/**".into()],
        },
    )
    .await;
    let session = create_session(&mut socket).await;
    attach(&mut socket, &session).await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session,
            text: "remember composer".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;

    assert!(events.iter().any(|e| matches!(e,
        ServerMessage::PageChanged { url, kind: PageChangeKind::Created, by: ChangedBy::Agent }
        if url == "site://memory/projects/composer.md")));
    let hits = server
        .index
        .read()
        .unwrap()
        .search(&composer_core::Query::parse("kind:project ratatui"));
    assert_eq!(hits.len(), 1);

    // The watcher sees our own write but must not report it as an external change.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    send(&mut socket, 10, ClientCommand::SessionList).await;
    let more = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 10, .. })
    })
    .await;
    assert!(
        !more.iter().any(|e| matches!(
            e,
            ServerMessage::PageChanged {
                by: ChangedBy::External,
                ..
            }
        )),
        "{more:?}"
    );
}

#[tokio::test]
async fn reattach_replays_without_deltas() {
    let (_dir, server, _provider, addr) = start(vec![vec![
        ModelEvent::Content("Hi".into()),
        ModelEvent::Content("!".into()),
        finished(vec![]),
    ]])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let session = create_session(&mut socket).await;
    attach(&mut socket, &session).await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: session.clone(),
            text: "hello".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;

    let mut second = connect(addr, &server.token).await;
    send(
        &mut second,
        1,
        ClientCommand::SessionAttach {
            session,
            since: None,
        },
    )
    .await;
    let replay = until(&mut second, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    assert!(
        !replay
            .iter()
            .any(|e| matches!(e, ServerMessage::MessageDelta { .. }))
    );
    let texts: Vec<_> = replay
        .iter()
        .filter_map(|e| match e {
            ServerMessage::MessageDone { message, .. } => Some(message.content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["hello", "Hi!"]);
}

#[tokio::test]
async fn earlier_thinking_is_not_resent() {
    let (_dir, server, provider, addr) = start(vec![
        vec![
            ModelEvent::Thinking("first thoughts".into()),
            ModelEvent::Content("one".into()),
            finished(vec![]),
        ],
        vec![ModelEvent::Content("two".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let session = create_session(&mut socket).await;
    attach(&mut socket, &session).await;
    for (id, text) in [(3, "a"), (4, "b")] {
        send(
            &mut socket,
            id,
            ClientCommand::MessageSend {
                session: session.clone(),
                text: text.into(),
                pages: vec![],
                images: vec![],
                attachments: vec![],
            },
        )
        .await;
        until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    }
    let second = &provider.requests()[1];
    assert!(second.messages.iter().all(|m| !matches!(
        m,
        ModelMessage::Assistant {
            thinking: Some(_),
            ..
        }
    )));
}

#[tokio::test]
async fn http_pages_auth_and_conflicts() {
    let (_dir, server, _provider, addr) = start(vec![]).await;
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    let info: serde_json::Value = client
        .get(format!("{base}/api/info"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(info["model"], "mock");
    assert_eq!(info["slots"], 2);
    assert!(
        !info["name"].as_str().unwrap().is_empty() && !info["version"].as_str().unwrap().is_empty()
    );

    let unauthorized = client
        .get(format!("{base}/api/pages/memory/user"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);

    let page: serde_json::Value = client
        .get(format!("{base}/api/pages/memory/user"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["url"], "site://memory/user.md");
    assert_eq!(page["frontmatter"]["kind"], "user");
    let hash = page["hash"].as_str().unwrap().to_string();

    let put = |if_match: &str| {
        client
            .put(format!("{base}/api/pages/memory/user"))
            .bearer_auth(&server.token)
            .json(&json!({ "content": "# User\nlikes dark themes\n", "if_match": if_match }))
            .send()
    };
    assert_eq!(put(&hash).await.unwrap().status(), 200);
    assert_eq!(
        put(&hash).await.unwrap().status(),
        409,
        "stale hash must conflict"
    );

    let cross = client
        .get(format!("{base}/api/sites"))
        .bearer_auth(&server.token)
        .header("Origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);

    let sites: Vec<serde_json::Value> = client
        .get(format!("{base}/api/sites?token={}", server.token))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        sites
            .iter()
            .any(|s| s["name"] == "orchestration" && s["has_justfile"] == true)
    );
}

#[tokio::test]
async fn a_session_model_and_its_history() {
    let reply = |text: &str| vec![ModelEvent::Content(text.into()), finished(vec![])];
    let (_dir, server, provider, addr) = start(vec![reply("one"), reply("two")]).await;
    let mut socket = connect(addr, &server.token).await;
    let session = create_session(&mut socket).await;
    attach(&mut socket, &session).await;

    // The mock lists no models: only its own.
    send(&mut socket, 3, ClientCommand::ModelList).await;
    let listed = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 3, .. })
    })
    .await;
    assert!(matches!(
        listed.last(),
        Some(ServerMessage::Ok { result: CommandResult::Models { default_model, models }, .. })
            if default_model == "mock" && models.is_empty()
    ));

    let set = |model: Option<&str>| ClientCommand::ModelSet {
        session: session.clone(),
        model: model.map(str::to_string),
    };
    send(&mut socket, 4, set(Some("openai/gpt-oss-120b"))).await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 4, .. })
    })
    .await;
    assert_eq!(
        server
            .sessions
            .get(&session)
            .unwrap()
            .info()
            .model
            .as_deref(),
        Some("openai/gpt-oss-120b")
    );
    for (id, text) in [(5, "first"), (7, "second")] {
        if id == 7 {
            // Back to the server's.
            send(&mut socket, 6, set(Some(" "))).await;
            until(&mut socket, |e| {
                matches!(e, ServerMessage::Ok { id: 6, .. })
            })
            .await;
        }
        send(
            &mut socket,
            id,
            ClientCommand::MessageSend {
                session: session.clone(),
                text: text.into(),
                pages: vec![],
                images: vec![],
                attachments: vec![],
            },
        )
        .await;
        until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    }
    let models: Vec<_> = provider.requests().into_iter().map(|r| r.model).collect();
    assert_eq!(models, [Some("openai/gpt-oss-120b".to_string()), None]);

    // The last message and what follows; then the ones before it.
    let history = |before| ClientCommand::SessionHistory {
        session: session.clone(),
        before,
        messages: 1,
    };
    send(&mut socket, 8, history(None)).await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 8, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::History { events, more: true },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    let said = |events: &[ServerMessage]| {
        events
            .iter()
            .filter_map(|e| match e {
                ServerMessage::MessageDone { message, .. } => Some(message.content.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(said(events), ["two"]);
    let ServerMessage::MessageDone { seq, .. } = &events[0] else {
        panic!("starts at a message: {events:?}")
    };
    send(&mut socket, 9, history(Some(*seq))).await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 9, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::History { events, .. },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    assert_eq!(said(events), ["second"]);
}

#[tokio::test]
async fn a_long_turn_resends_only_its_last_steps_thinking() {
    // Four steps of one turn, each thinking before an `open`; then the answer.
    let step = |n: u32| {
        vec![
            ModelEvent::Thinking(format!("plan {n}")),
            finished(vec![tool_call(
                &format!("c{n}"),
                "open",
                json!({ "url": "site://memory/user" }),
            )]),
        ]
    };
    let (_dir, server, provider, addr) = start(vec![
        step(1),
        step(2),
        step(3),
        step(4),
        vec![ModelEvent::Content("done".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let session = create_session(&mut socket).await;
    attach(&mut socket, &session).await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: session.clone(),
            text: "go".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let last = &provider.requests()[4];
    let thoughts: Vec<_> = last
        .messages
        .iter()
        .filter_map(|m| match m {
            ModelMessage::Assistant {
                thinking: Some(t), ..
            } => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(thoughts, ["plan 3", "plan 4"]);
}
