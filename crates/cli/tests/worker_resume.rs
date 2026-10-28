//! A composer worker the server stops (a shutdown, a restart) pauses instead of failing, and a
//! message goes on from where it was: the same session, its history, then the message.

mod support;

use std::collections::VecDeque;
use std::time::Duration;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage, WorkerState};
use composer_server::{Home, Server, init_home};
use serde_json::json;
use support::{Reply, Scripts, fake_model_logged, send, until};

#[tokio::test]
async fn a_stopped_worker_goes_on_from_its_history() {
    let (base_url, requests) = fake_model_logged(Scripts {
        orchestrator: VecDeque::from([
            Reply::Call("spawn_worker", json!({ "ticket": "0001" })),
            Reply::Text("worker started"),
            Reply::Text("ticket 0001 is done"),
            // A finding comes back to the same worker.
            Reply::Call(
                "continue_worker",
                json!({ "ticket": "0001", "message": "rename x to y" }),
            ),
            Reply::Text("sent it back"),
            Reply::Text("ticket 0001 is done again"),
        ]),
        // It reads Home, then works on until the server stops; resumed, it finishes.
        worker: VecDeque::from([
            Reply::Call("open", json!({ "url": "site://home" })),
            Reply::Hang,
            Reply::Call("finish", json!({ "status": "done", "summary": "went on" })),
            Reply::Text("finished"),
            Reply::Call("finish", json!({ "status": "done", "summary": "renamed" })),
            Reply::Text("finished again"),
        ]),
        ..Scripts::default()
    })
    .await;

    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    std::fs::create_dir_all(dir.path().join("sites/orchestration/tickets")).unwrap();
    let ticket_file = dir.path().join("sites/orchestration/tickets/0001.md");
    std::fs::write(
        &ticket_file,
        "---\ntitle: Read\ntype: ticket\nid: \"0001\"\nstatus: open\nagent: worker\n---\n# Read\n\n## Task\nRead Home.\n\n## Result\n",
    )
    .unwrap();
    std::fs::write(
        home.config_file(),
        format!("[provider]\nbase_url = \"{base_url}\"\n"),
    )
    .unwrap();
    // SAFETY: set before any server or thread reads it; this test is the only one in this binary.
    unsafe { std::env::set_var("COMPOSER_WORKER_EXE", env!("CARGO_BIN_EXE_composer")) };
    let server = Server::start(home).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = server.clone();
    tokio::spawn(async move { serving.serve(listener).await });

    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws?token={}", server.token))
            .await
            .unwrap();
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "orchestrator".into(),
            project: None,
        },
    )
    .await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!()
    };
    let orchestrator = session.id.clone();
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: orchestrator.clone(),
            since: None,
        },
    )
    .await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: orchestrator.clone(),
            text: "do it".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;

    // It has read Home and hangs on its next call: stop the server's workers.
    let events = until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::ToolResult { .. })
    })
    .await;
    let worker = events
        .iter()
        .find_map(|e| match e {
            ServerMessage::WorkerStatus { worker, .. } => Some(worker.clone()),
            _ => None,
        })
        .expect("a worker started");
    tokio::time::sleep(Duration::from_millis(300)).await;
    server.workers.shutdown().await;
    let ticket = std::fs::read_to_string(&ticket_file).unwrap();
    assert!(ticket.contains("status: paused"), "{ticket}");
    assert!(
        ticket.contains(&format!("harness_session: {worker}")),
        "{ticket}"
    );
    assert_eq!(
        server.sessions.get(&worker).unwrap().info().worker,
        Some(WorkerState::Paused)
    );

    // A message goes on: the same session, its history first.
    send(
        &mut socket,
        4,
        ClientCommand::MessageSend {
            session: worker.clone(),
            text: "go on, the server is back".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::MessageDone { session, message, .. }
            if *session == orchestrator && message.content == "ticket 0001 is done")
    })
    .await;
    let ticket = std::fs::read_to_string(&ticket_file).unwrap();
    assert!(ticket.contains("status: done"), "{ticket}");

    let resumed = requests
        .lock()
        .unwrap()
        .iter()
        .find(|r| r.to_string().contains("go on, the server is back"))
        .cloned()
        .expect("the worker was asked again");
    let text = resumed.to_string();
    assert!(
        text.contains("Your ticket: site://orchestration/tickets/0001.md"),
        "{text}"
    );
    assert!(text.contains("\"open\""), "the call it made before: {text}");

    // Finished, it goes back to its ticket when the orchestrator sends it a finding.
    send(
        &mut socket,
        5,
        ClientCommand::MessageSend {
            session: orchestrator.clone(),
            text: "the review found a nit".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::MessageDone { session, message, .. }
            if *session == orchestrator && message.content == "ticket 0001 is done again")
    })
    .await;
    let back = requests
        .lock()
        .unwrap()
        .iter()
        .find(|r| {
            r.to_string()
                .contains("From your orchestrator, about your ticket: rename x to y")
        })
        .cloned()
        .expect("the finding reached the worker");
    assert!(
        back.to_string().contains("go on, the server is back"),
        "with its history: {back}"
    );
}
