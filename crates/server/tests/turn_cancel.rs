//! A Stop is for the turn the client had seen. Pressed twice while a queued message was waiting,
//! the second one used to stop that message's turn the moment it started (four times in the
//! transcripts up to 2026-11-17).

mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use support::*;

fn message(session: &str, text: &str) -> ClientCommand {
    ClientCommand::MessageSend {
        session: session.into(),
        text: text.into(),
        pages: vec![],
        images: vec![],
        attachments: vec![],
    }
}

fn started(event: &ServerMessage) -> Option<u64> {
    match event {
        ServerMessage::TurnStarted { seq, .. } => Some(*seq),
        _ => None,
    }
}

#[tokio::test]
async fn a_second_stop_leaves_the_queued_turn_it_had_not_seen() {
    let (_dir, server, provider, addr) = start(vec![]).await;
    // Every model call hangs: a turn runs until it is stopped.
    provider.stalled.store(true, Ordering::SeqCst);
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

    send(&mut socket, 3, message(&session, "first")).await;
    let first = until(&mut socket, |e| started(e).is_some()).await;
    let seen = first.iter().find_map(started).unwrap();
    send(&mut socket, 4, message(&session, "queued")).await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::TurnQueued { .. })
    })
    .await;

    // Stop, twice, both from what the client had seen: the first turn.
    let stop = |id| {
        (
            id,
            ClientCommand::TurnCancel {
                session: session.clone(),
                seen: Some(seen),
            },
        )
    };
    let (id, command) = stop(5);
    send(&mut socket, id, command).await;
    let events = until(&mut socket, |e| started(e).is_some()).await;
    assert!(events.iter().any(|e| matches!(
        e,
        ServerMessage::TurnDone {
            cancelled: true,
            ..
        }
    )));
    let queued = events.iter().find_map(started).unwrap();
    let (id, command) = stop(6);
    send(&mut socket, id, command).await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 6, .. })
    })
    .await;

    // The queued turn is still running.
    let more = tokio::time::timeout(Duration::from_millis(300), async {
        until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await
    })
    .await;
    assert!(more.is_err(), "the queued turn was stopped: {more:?}");
    assert!(server.sessions.get(&session).unwrap().is_running());

    // A Stop that has seen it stops it.
    send(
        &mut socket,
        7,
        ClientCommand::TurnCancel {
            session: session.clone(),
            seen: Some(queued),
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(
            e,
            ServerMessage::TurnDone {
                cancelled: true,
                ..
            }
        )
    })
    .await;
}
