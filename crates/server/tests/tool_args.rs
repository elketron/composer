//! A garbled call from a local model's chat template reaches the tool the way its schema says.

mod support;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::provider::ModelEvent;
use serde_json::json;
use support::*;

#[tokio::test]
async fn wrapped_and_leaked_arguments_reach_the_tool_repaired() {
    let (_dir, server, _provider, addr) = start(vec![
        vec![finished(vec![
            // Wrapped once more, as a JSON string.
            tool_call(
                "c1",
                "open",
                json!({ "arguments": "{\"url\": \"site://memory/user\"}" }),
            ),
            // Its XML format leaking into the value.
            tool_call(
                "c2",
                "open",
                json!({ "url": "<parameter=url>\nsite://memory/user\n</parameter>" }),
            ),
            // Not an argument of `open`: said back when the call fails.
            tool_call("c3", "open", json!({ "path": "site://memory/user" })),
        ])],
        vec![ModelEvent::Content("ok".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            kind: composer_protocol::RunKind::Agentic,
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
        panic!("no session: {reply:?}")
    };
    let session = session.id.clone();
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: session.clone(),
            since: None,
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session,
            text: "go".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let results: Vec<(String, bool)> = events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result.clone(), *is_error)),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 3, "{results:?}");
    for (result, is_error) in &results[..2] {
        assert!(
            !is_error && result.starts_with("url: site://memory/user.md"),
            "{result}"
        );
    }
    let (failed, is_error) = &results[2];
    assert!(
        *is_error && failed.contains("(not arguments of `open`, ignored: `path`; it takes `url`"),
        "{failed}"
    );
}
