//! Compaction: memory pass, summary, history replaced; and the memory-aware prompt.

mod support;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use composer_server::provider::{ModelEvent, ModelMessage, Usage};
use serde_json::json;
use support::*;

fn answer(text: &str, prompt_tokens: u32) -> Vec<ModelEvent> {
    vec![
        ModelEvent::Content(text.into()),
        ModelEvent::Finished {
            tool_calls: vec![],
            usage: Some(Usage {
                prompt_tokens,
                completion_tokens: 10,
            }),
            finish_reason: None,
        },
    ]
}

async fn session(socket: &mut Socket) -> String {
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
        panic!()
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
    session.id.clone()
}

async fn say(socket: &mut Socket, session: &str, id: u64, text: &str) -> Vec<ServerMessage> {
    send(
        socket,
        id,
        ClientCommand::MessageSend {
            session: session.into(),
            text: text.into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    until(socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await
}

fn last_user(messages: &[ModelMessage]) -> &str {
    messages
        .iter()
        .rev()
        .find_map(|m| match m {
            ModelMessage::User { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .unwrap()
}

#[tokio::test]
async fn context_over_threshold_saves_memory_then_summarizes() {
    let mut config = Config::defaults();
    config.agent.compaction_threshold = 1000;
    let memory = "---\ntitle: Compaction\ntype: memory\nkind: topic\n---\n# Compaction\n- tested\n";
    let (dir, server, provider, addr) = start_with(
        config,
        vec![
            answer("first answer", 5000),
            // memory pass: one write, then done
            vec![finished(vec![tool_call(
                "m1",
                "write",
                json!({ "url": "site://memory/topics/compaction", "content": memory }),
            )])],
            vec![
                ModelEvent::Content("saved the compaction topic".into()),
                finished(vec![]),
            ],
            // summary
            vec![
                ModelEvent::Content("SUMMARY: the user asked a first question.".into()),
                finished(vec![]),
            ],
            // the actual second turn
            answer("second answer", 300),
        ],
    )
    .await;
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    say(&mut socket, &id, 3, "first question").await;
    let events = say(&mut socket, &id, 4, "second question").await;

    // The memory pass is visible, then the compaction event.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::ToolCallStarted { name, .. } if name == "write"))
    );
    let compacted = events.iter().find_map(|e| match e {
        ServerMessage::SessionCompacted {
            summary, messages, ..
        } => Some((summary.clone(), *messages)),
        _ => None,
    });
    assert_eq!(
        compacted,
        Some(("SUMMARY: the user asked a first question.".into(), 2))
    );
    assert!(
        dir.path()
            .join("sites/memory/topics/compaction.md")
            .is_file()
    );

    let requests = provider.requests();
    assert_eq!(requests.len(), 5);
    let memory_pass = &requests[1];
    let mut tool_names: Vec<_> = memory_pass.tools.iter().map(|t| t.name.as_str()).collect();
    tool_names.sort();
    assert_eq!(tool_names, ["edit", "open", "search", "write"]);
    assert!(last_user(&memory_pass.messages).contains("nearly full"));
    // The memory pass sees the old conversation, but not the new question.
    assert!(memory_pass.messages.iter().any(
        |m| matches!(m, ModelMessage::Assistant { content, .. } if content == "first answer")
    ));
    assert!(
        !memory_pass.messages.iter().any(
            |m| matches!(m, ModelMessage::User { content, .. } if content == "second question")
        )
    );

    let summary_call = &requests[3];
    assert!(summary_call.tools.is_empty());
    assert!(last_user(&summary_call.messages).starts_with("[composer] Summarize"));

    // The turn continues on the compacted history.
    let turn = &requests[4];
    let users: Vec<&str> = turn
        .messages
        .iter()
        .filter_map(|m| match m {
            ModelMessage::User { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(users.len(), 2);
    assert!(users[0].contains("SUMMARY: the user asked a first question."));
    assert_eq!(users[1], "second question");
    assert!(!turn.messages.iter().any(
        |m| matches!(m, ModelMessage::Assistant { content, .. } if content == "first answer")
    ));

    // The old history is archived next to the new one.
    let session_dir = dir.path().join("state/sessions").join(&id);
    let archived = std::fs::read_dir(&session_dir)
        .unwrap()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("history.") && name != "history.jsonl"
        });
    assert_eq!(archived.count(), 1);
    assert!(server.sessions.get(&id).unwrap().info().context_tokens < 1000);
}

#[tokio::test]
async fn manual_compaction_between_turns() {
    let (_dir, server, provider, addr) = start(vec![
        answer("hello there", 200),
        vec![
            ModelEvent::Content("nothing worth saving".into()),
            finished(vec![]),
        ],
        vec![
            ModelEvent::Content("SUMMARY: greetings were exchanged.".into()),
            finished(vec![]),
        ],
        answer("still here", 100),
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    say(&mut socket, &id, 3, "hi").await;

    send(
        &mut socket,
        4,
        ClientCommand::SessionCompact {
            session: id.clone(),
        },
    )
    .await;
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::SessionCompacted { .. })
    })
    .await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::SessionCompacted { messages: 2, .. }))
    );

    say(&mut socket, &id, 5, "are you there?").await;
    let turn = provider.requests().pop().unwrap();
    assert!(
        matches!(&turn.messages[1], ModelMessage::User { content, .. } if content.contains("greetings were exchanged"))
    );
    assert!(
        !turn
            .messages
            .iter()
            .any(|m| matches!(m, ModelMessage::User { content, .. } if content == "hi"))
    );
}

#[tokio::test]
async fn prompt_names_the_agent_and_lists_memory_pages() {
    let (_dir, server, provider, addr) = start(vec![answer("ok", 100)]).await;
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    say(&mut socket, &id, 3, "hi").await;
    let ModelMessage::System { content } = &provider.requests()[0].messages[0] else {
        panic!()
    };
    assert!(
        content.contains("you are the `default` agent (site://agents/default.md)"),
        "{content}"
    );
    assert!(
        content.contains("- User — site://memory/user.md (user)"),
        "{content}"
    );
    assert!(
        !content.contains("site://memory/index.md ("),
        "the index itself is not listed"
    );
}
