//! `spawn_run` from an agentic session: a single shot and a workflow run give their value back as
//! the tool's result; an agentic run is a stored child session whose end is posted into its
//! parent, or, with `wait`, whose answer is the result.

mod support;

use composer_protocol::{
    ChangedBy, ClientCommand, CommandResult, RunKind, ServerMessage, SessionInfo,
};
use composer_server::Server;
use composer_server::provider::{ModelEvent, ModelMessage};
use serde_json::json;
use support::*;

fn write_prompts_site(server: &Server) {
    std::fs::create_dir_all(server.home.sites().join("prompts")).unwrap();
    let page = |path: &str, text: &str| {
        let url = composer_core::SiteUrl::new("prompts", path).unwrap();
        server.write_page(&url, text, ChangedBy::External).unwrap();
    };
    page("index.md", "---\ntitle: Prompts\n---\nThe prompts site.\n");
    page("krea/CONTEXT.md", "Rewrite the prompt for krea2.\n");
}

/// An agentic session, attached, that has been sent `text`.
async fn parent(socket: &mut Socket, text: &str) -> SessionInfo {
    send(
        socket,
        1,
        ClientCommand::SessionCreate {
            kind: RunKind::Agentic,
            model: None,
            site: None,
            harness: None,
        },
    )
    .await;
    let reply = until(socket, |e| matches!(e, ServerMessage::Ok { id: 1, .. })).await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
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
    send(
        socket,
        3,
        ClientCommand::MessageSend {
            session: session.id.clone(),
            text: text.into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    session.clone()
}

fn tool_results(events: &[ServerMessage]) -> Vec<(String, bool)> {
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

#[tokio::test]
async fn a_single_shot_and_a_workflow_run_return_their_values() {
    let (_dir, server, provider, addr) = start(vec![
        vec![finished(vec![tool_call(
            "c1",
            "spawn_run",
            json!({ "kind": "single_shot", "site": "prompts", "folder": "krea", "prompt": "a cat" }),
        )])],
        vec![ModelEvent::Content("a fluffy cat".into()), finished(vec![])],
        vec![finished(vec![tool_call(
            "c2",
            "spawn_run",
            json!({ "kind": "workflow", "prompt": "count" }),
        )])],
        vec![finished(vec![tool_call(
            "c3",
            "finish",
            json!({ "value": { "count": 3 } }),
        )])],
        vec![ModelEvent::Content("ok".into()), finished(vec![])],
    ])
    .await;
    write_prompts_site(&server);
    let mut socket = connect(addr, &server.token).await;
    parent(&mut socket, "go").await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert_eq!(
        tool_results(&events),
        [
            ("a fluffy cat".to_string(), false),
            (r#"{"count":3}"#.to_string(), false)
        ]
    );

    let requests = provider.requests();
    // The agentic run has spawn_run; the workflow run it spawned does not.
    assert!(requests[0].tools.iter().any(|t| t.name == "spawn_run"));
    assert!(!requests[3].tools.iter().any(|t| t.name == "spawn_run"));
    // The single shot: its folder's instructions, then its input.
    match &requests[1].messages[..] {
        [
            ModelMessage::System { content },
            ModelMessage::User { content: input, .. },
        ] => {
            assert_eq!(content, "Rewrite the prompt for krea2.");
            assert_eq!(input, "a cat");
        }
        other => panic!("{other:?}"),
    }
    // Neither was stored.
    assert_eq!(server.sessions.list().len(), 1);
}

#[tokio::test]
async fn an_agentic_child_tells_its_parent_when_it_ends() {
    let (_dir, server, _provider, addr) = start(vec![
        vec![finished(vec![tool_call(
            "c1",
            "spawn_run",
            json!({ "kind": "agentic", "prompt": "review the idea" }),
        )])],
        // The parent's next step and the child's run race for these two: both just answer.
        vec![ModelEvent::Content("done".into()), finished(vec![])],
        vec![ModelEvent::Content("done".into()), finished(vec![])],
        // The parent's turn on the child's note.
        vec![ModelEvent::Content("noted".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let parent = parent(&mut socket, "spawn a reviewer").await;
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::MessageDone { message, session, .. }
            if *session == parent.id && message.content == "noted")
    })
    .await;
    let (started, _) = &tool_results(&events)[0];
    let child = started
        .strip_prefix("started session ")
        .and_then(|rest| rest.split(';').next())
        .unwrap_or_else(|| panic!("{started}"))
        .to_string();
    let info = server.sessions.get(&child).unwrap().info();
    assert_eq!(info.parent.as_deref(), Some(parent.id.as_str()));
    assert_eq!(info.kind, RunKind::Agentic);

    let note = format!("[spawned run {child} ended] done");
    assert!(
        events.iter().any(|e| matches!(e,
            ServerMessage::MessageDone { message, session, .. }
                if *session == parent.id && message.content == note)),
        "{events:#?}"
    );
}

#[tokio::test]
async fn an_agentic_child_waited_for_returns_its_answer() {
    let (_dir, server, _provider, addr) = start(vec![
        vec![finished(vec![tool_call(
            "c1",
            "spawn_run",
            json!({ "kind": "agentic", "prompt": "say hi", "wait": true }),
        )])],
        vec![
            ModelEvent::Content("hi from the child".into()),
            finished(vec![]),
        ],
        vec![ModelEvent::Content("ok".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let parent = parent(&mut socket, "go").await;
    let events = until(
        &mut socket,
        |e| matches!(e, ServerMessage::TurnDone { session, .. } if *session == parent.id),
    )
    .await;
    let (result, is_error) = &tool_results(&events)[0];
    assert!(!is_error);
    assert!(result.ends_with("answered:\nhi from the child"), "{result}");
    // Waited for: no note was posted into the parent.
    let history = server.sessions.get(&parent.id).unwrap().history();
    assert!(
        !history.iter().any(|m| matches!(m,
            ModelMessage::User { content, .. } if content.starts_with("[spawned run"))),
        "{history:?}"
    );
    assert_eq!(server.sessions.list().len(), 2);
}
