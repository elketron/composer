//! A worker process that reaches its token limit doesn't fail: it waits, its orchestrator is
//! told what it was doing and continues it with a message (compacting its session first), and it
//! finishes the ticket.

mod support;

use std::collections::VecDeque;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage, WorkerState};
use composer_server::{Home, Server, init_home};
use serde_json::json;
use support::{Reply, Scripts, fake_model_logged, send, until};

#[tokio::test]
async fn the_orchestrator_continues_a_worker_at_its_turn_limit() {
    let (base_url, requests) = fake_model_logged(Scripts {
        orchestrator: VecDeque::from([
            Reply::Call("spawn_worker", json!({ "ticket": "0001" })),
            Reply::Text("worker started"),
            Reply::Call(
                "continue_worker",
                json!({ "ticket": "0001", "message": "you have read enough; write the Result and finish", "compact": true }),
            ),
            Reply::Text("continued it"),
            Reply::Text("ticket 0001 is done"),
        ]),
        // Reading Home (~500 tokens) uses up the budget of 300; the orchestrator's small calls
        // stay under it. After the message the worker finishes.
        worker: VecDeque::from([
            Reply::Call("open", json!({ "url": "site://home" })),
            Reply::Call("finish", json!({ "status": "done", "summary": "read and done" })),
            Reply::Text("finished"),
        ]),
        ..Scripts::default()
    })
    .await;

    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    std::fs::create_dir_all(dir.path().join("sites/orchestration/tickets")).unwrap();
    std::fs::write(
        dir.path().join("sites/orchestration/tickets/0001.md"),
        "---\ntitle: Read\ntype: ticket\nid: \"0001\"\nstatus: open\nagent: worker\n---\n# Read\n\n## Task\nRead Home.\n\n## Result\n",
    )
    .unwrap();
    std::fs::write(
        home.config_file(),
        format!("[provider]\nbase_url = \"{base_url}\"\n\n[agent]\nturn_tokens = 300\n"),
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

    let events = until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::MessageDone { session, message, .. }
            if *session == orchestrator && message.content == "ticket 0001 is done")
    })
    .await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            ServerMessage::WorkerStatus {
                state: WorkerState::Stuck,
                ..
            }
        )),
        "the worker waited at its limit"
    );
    let told = events.iter().find_map(|e| match e {
        ServerMessage::MessageDone {
            session, message, ..
        } if *session == orchestrator && message.content.contains("reached its turn limit") => {
            Some(message.content.clone())
        }
        _ => None,
    });
    let told = told.expect("the orchestrator was told");
    assert!(told.contains("- open("), "{told}");
    assert!(told.contains("continue_worker(0001"), "{told}");
    assert!(told.contains("compact: true"), "{told}");

    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(ticket.contains("status: done"), "{ticket}");
    assert!(ticket.contains("continued: 1"), "{ticket}");

    // The worker's next round started from the orchestrator's message, on the summary instead
    // of its old steps.
    let requests = requests.lock().unwrap();
    let next = requests
        .iter()
        .find(|r| {
            r["messages"].as_array().unwrap().iter().any(|m| {
                m["content"].as_str().is_some_and(|c| {
                    c.contains(
                        "From your orchestrator, after your turn limit: you have read enough",
                    )
                })
            })
        })
        .expect("the worker got the message");
    assert!(
        !next["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "tool"),
        "its old tool results were compacted away: {next}"
    );
    // The compaction's summary: a call without tools.
    assert!(
        requests
            .iter()
            .any(|r| r["tools"].as_array().is_none_or(|t| t.is_empty())),
        "a summary was written"
    );
}
