//! A model reply that breaks off or comes back empty: the turn makes the call again by itself,
//! and when that keeps failing it stops with the error instead of ending quietly (2026-11-17: an
//! orchestrator went silent after `set_project` when the provider cut its stream).
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use composer_protocol::{ClientCommand, CommandResult, Role, ServerMessage};
use composer_server::provider::ModelEvent;
use serde_json::json;
use support::*;

/// A session, attached, with one message sent: the events up to the turn's end, and how many
/// model calls were made.
async fn one_turn(scripts: Vec<Vec<ModelEvent>>) -> (Vec<ServerMessage>, usize) {
    let (_dir, server, provider, addr) = start(scripts).await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "orchestrator".into(),
            project: None,
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
    (events, provider.requests().len())
}

fn said(events: &[ServerMessage], role: Role) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::MessageDone { message, .. } if message.role == role => {
                Some(message.content.clone())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_broken_off_reply_after_a_tool_call_continues_by_itself() {
    let (events, calls) = one_turn(vec![
        vec![finished(vec![tool_call(
            "c1",
            "open",
            json!({ "url": "site://memory/user" }),
        )])],
        // Empty: no text, no thinking, no tool call.
        vec![finished(vec![])],
        // Cut off: the stream closes before it finishes.
        vec![ModelEvent::Thinking("half a tho".into())],
        vec![ModelEvent::Content("Done.".into()), finished(vec![])],
    ])
    .await;

    assert_eq!(calls, 4);
    let notices = said(&events, Role::System);
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert!(
        notices[0].starts_with("the model sent an empty reply (stop reason: none)")
            && notices[0].ends_with("continuing by itself (1 of 2)"),
        "{notices:?}"
    );
    assert!(
        notices[1].starts_with("model stream ended unexpectedly")
            && notices[1].ends_with("(2 of 2)"),
        "{notices:?}"
    );
    assert_eq!(said(&events, Role::Assistant).last().unwrap(), "Done.");
    assert!(matches!(
        events.last(),
        Some(ServerMessage::TurnDone {
            cancelled: false,
            ..
        })
    ));
}

#[tokio::test]
async fn a_reply_that_stays_empty_stops_the_turn_with_the_error() {
    let (events, calls) = one_turn(vec![
        vec![finished(vec![])],
        vec![finished(vec![])],
        vec![finished(vec![])],
    ])
    .await;

    assert_eq!(calls, 3, "the call and two more, then it stops");
    let notices = said(&events, Role::System);
    assert_eq!(
        notices.last().unwrap(),
        "Turn stopped: the model sent an empty reply (stop reason: none)"
    );
    assert!(said(&events, Role::Assistant).is_empty());
}
