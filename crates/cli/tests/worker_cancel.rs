//! Cancelling a running worker stops its process; the ticket fails as cancelled and the
//! orchestrator is told. A running worker refuses messages (its turns run in its harness).

mod support;

use composer_protocol::{ClientCommand, ServerMessage, WorkerState};
use support::{BusyWorker, busy_worker, send, until};

#[tokio::test]
async fn cancelling_a_worker_stops_it_and_fails_the_ticket() {
    let Some(BusyWorker {
        dir,
        server,
        mut socket,
        orchestrator,
        worker,
        ..
    }) = busy_worker("noted", false).await
    else {
        return;
    };

    send(
        &mut socket,
        5,
        ClientCommand::MessageSend {
            session: worker.clone(),
            text: "hurry up".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Error { id: Some(5), .. })
    })
    .await;
    assert!(
        matches!(events.last(), Some(ServerMessage::Error { message, .. })
        if message.contains("can't take messages while it works"))
    );

    send(
        &mut socket,
        6,
        ClientCommand::TurnCancel {
            session: worker.clone(),
        },
    )
    .await;
    let events = until(&mut socket, 30, |e| {
        matches!(e, ServerMessage::MessageDone { session, message, .. }
            if *session == orchestrator && message.content == "noted")
    })
    .await;
    assert!(events.iter().any(|e| matches!(
        e,
        ServerMessage::WorkerStatus {
            state: WorkerState::Failed,
            ..
        }
    )));
    assert!(events.iter().any(
        |e| matches!(e, ServerMessage::MessageDone { session, message, .. }
        if *session == orchestrator && message.content.contains("finished ticket 0001: failed")
            && message.content.contains("cancelled by the user"))
    ));
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(ticket.contains("status: failed"));
    assert!(server.workers.running().is_empty());
    assert_eq!(
        server.sessions.get(&worker).unwrap().info().worker,
        Some(WorkerState::Failed)
    );
}
