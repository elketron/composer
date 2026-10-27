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
async fn a_long_turn_keeps_a_prompt_that_only_grows() {
    // Six steps of one turn, each thinking about 6K tokens before an `open`; then the answer.
    // With the default threshold a segment holds 20K tokens of thinking: three steps.
    let step = |n: u32| {
        vec![
            ModelEvent::Thinking(format!("plan {n} {}", "x".repeat(24_000))),
            finished(vec![tool_call(
                &format!("c{n}"),
                "open",
                json!({ "url": "site://memory/user" }),
            )]),
        ]
    };
    let mut scripts: Vec<_> = (1..=6).map(step).collect();
    scripts.push(vec![ModelEvent::Content("done".into()), finished(vec![])]);
    let (_dir, server, provider, addr) = start(scripts).await;
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
    let requests = provider.requests();
    let thoughts = |n: usize| -> Vec<String> {
        requests[n]
            .messages
            .iter()
            .filter_map(|m| match m {
                ModelMessage::Assistant {
                    thinking: Some(t), ..
                } => Some(t.split(' ').take(2).collect::<Vec<_>>().join(" ")),
                _ => None,
            })
            .collect()
    };
    // A prompt cache resumes an exact prefix: each request starts with the one before it,
    // except where a segment filled (plan 4 starts the second; the first goes).
    let prefix = |a: usize, b: usize| requests[b].messages.starts_with(&requests[a].messages);
    assert!(prefix(1, 2) && prefix(2, 3) && prefix(4, 5) && prefix(5, 6));
    assert!(!prefix(3, 4));
    assert_eq!(thoughts(3), ["plan 1", "plan 2", "plan 3"]);
    assert_eq!(thoughts(6), ["plan 4", "plan 5", "plan 6"]);
}

/// A 2x2 red PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd, 0xd4, 0x9a,
    0x73, 0x00, 0x00, 0x00, 0x13, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x9f, 0x01, 0x8c, 0xff, 0x33, 0x30, 0x00, 0x00, 0x1f, 0xee, 0x03, 0xfd, 0x35, 0x1b, 0x00, 0x33,
    0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[tokio::test]
async fn open_shows_an_image_to_the_model() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("build")).unwrap();
    std::fs::write(project.path().join("build/shot.png"), PNG).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("other.png"), PNG).unwrap();
    let calls = [
        json!({ "url": "build/shot.png" }),
        json!({ "url": "site://memory/pic.png" }),
        json!({ "url": outside.path().join("other.png") }),
        json!({ "url": "site://memory/user" }),
    ];
    let mut scripts: Vec<_> = calls
        .iter()
        .enumerate()
        .map(|(i, args)| {
            vec![finished(vec![tool_call(
                &format!("c{i}"),
                "open",
                args.clone(),
            )])]
        })
        .collect();
    scripts.push(vec![ModelEvent::Content("seen".into()), finished(vec![])]);
    let (dir, server, provider, addr) = start(scripts).await;
    std::fs::write(dir.path().join("sites/memory/pic.png"), PNG).unwrap();
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "default".into(),
            project: Some(project.path().display().to_string()),
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
    let session = session.id.clone();
    attach(&mut socket, &session).await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: session.clone(),
            text: "look".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let last = provider.requests().pop().unwrap();
    let tools: Vec<(&str, usize)> = last
        .messages
        .iter()
        .filter_map(|m| match m {
            ModelMessage::Tool {
                content, images, ..
            } => Some((content.as_str(), images.len())),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 4, "{tools:?}");
    assert!(
        tools[0].0.starts_with("build/shot.png: image (image/png"),
        "{tools:?}"
    );
    assert_eq!(tools[0].1, 1);
    assert!(
        tools[1].0.starts_with("site://memory/pic.png: image"),
        "{tools:?}"
    );
    assert_eq!(tools[1].1, 1);
    assert!(tools[2].0.contains("outside your project"), "{tools:?}");
    assert_eq!(tools[2].1, 0);
    assert!(
        tools[3].0.starts_with("url: site://memory/user"),
        "{tools:?}"
    );
    assert_eq!(tools[3].1, 0);
}
