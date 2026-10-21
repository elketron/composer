//! An orchestrator spawns a real worker process (this crate's `composer` binary) on a
//! ticket; both talk to a fake OpenAI-compatible model that answers by role.

mod support;

use std::collections::VecDeque;

use composer_protocol::{
    ClientCommand, CommandResult, MessageSource, Role, ServerMessage, WorkerState,
};
use composer_server::{Home, Server, init_home};
use serde_json::json;
use support::{Reply, Scripts, fake_model, send, until};

#[tokio::test]
async fn orchestrator_runs_a_worker_process_on_a_ticket() {
    if std::process::Command::new("just")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let note = "---\ntitle: Note\ntype: memory\nkind: topic\n---\n# Note\n- written by a worker\n";
    let base_url = fake_model(Scripts {
        orchestrator: VecDeque::from([
            Reply::Call("run", json!({ "site": "orchestration", "recipe": "new-ticket", "args": ["Write a note"] })),
            // Not a worker agent: refused, with the ones there are.
            Reply::Call("spawn_worker", json!({ "ticket": "1", "agent": "default" })),
            Reply::Call("spawn_worker", json!({ "ticket": "1", "agent": "reviewer" })),
            Reply::Text("worker started"),
            // after the worker's finish message:
            Reply::Text("ticket 0001 is done"),
        ]),
        worker: VecDeque::from([
            Reply::Call("write", json!({ "url": "site://memory/topics/note", "content": note })),
            Reply::Call("finish", json!({ "status": "done", "summary": "wrote the note" })),
            Reply::Text("finished"),
        ]),
        ..Scripts::default()
    })
    .await;

    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let worker = std::fs::read_to_string(dir.path().join("sites/agents/worker.md")).unwrap();
    std::fs::write(
        dir.path().join("sites/agents/reviewer.md"),
        worker.replace("name: worker", "name: reviewer").replace(
            "description: general worker; carries out any ticket",
            "description: reviews",
        ),
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
            text: "get a note written".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;

    // The worker starts; attach to it to watch it live.
    let events = until(&mut socket, 30, |e| {
        matches!(
            e,
            ServerMessage::WorkerStatus {
                state: WorkerState::Starting,
                ..
            }
        )
    })
    .await;
    let Some(ServerMessage::WorkerStatus {
        worker,
        ticket,
        parent,
        ..
    }) = events.last()
    else {
        panic!()
    };
    assert_eq!(
        (ticket.as_str(), parent.as_str()),
        ("0001", orchestrator.as_str())
    );
    let worker = worker.clone();
    assert!(events.iter().any(|e| matches!(e, ServerMessage::ToolResult { result, is_error: true, .. }
        if result.contains("`default` is not a worker agent") && result.contains("worker agents: reviewer, worker"))));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::SessionUpdated { session }
        if session.id == worker && session.agent == "reviewer"))
    );
    let ticket_file = dir.path().join("sites/orchestration/tickets/0001.md");
    assert!(
        std::fs::read_to_string(&ticket_file)
            .unwrap()
            .contains("status: claimed")
    );
    send(
        &mut socket,
        4,
        ClientCommand::SessionAttach {
            session: worker.clone(),
            since: None,
        },
    )
    .await;

    // Everything until the orchestrator's follow-up turn is done.
    let events = until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::MessageDone { session, message, .. }
            if *session == orchestrator && message.content == "ticket 0001 is done")
    })
    .await;

    // The worker's own tool calls arrived through the parent, in the worker session.
    assert!(events.iter().any(|e| matches!(e, ServerMessage::ToolCallStarted { session, name, .. } if *session == worker && name == "write")));
    assert!(events.iter().any(|e| matches!(
        e,
        ServerMessage::WorkerStatus {
            state: WorkerState::Done,
            ..
        }
    )));
    // The session keeps the outcome, for clients that connect later.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::SessionUpdated { session }
        if session.id == worker && session.worker == Some(WorkerState::Done)))
    );
    // The orchestrator was told, by a message from the worker.
    assert!(events.iter().any(
        |e| matches!(e, ServerMessage::MessageDone { session, message, .. }
        if *session == orchestrator && message.role == Role::User
            && message.source == Some(MessageSource::Worker { session: worker.clone() })
            && message.content.contains("finished ticket 0001: done"))
    ));

    assert!(
        std::fs::read_to_string(&ticket_file)
            .unwrap()
            .contains("status: done")
    );
    assert!(
        dir.path().join("sites/memory/topics/note.md").is_file(),
        "the worker wrote through the shared workspace"
    );
    let info = server.sessions.get(&worker).unwrap().info();
    assert_eq!(
        (info.ticket.as_deref(), info.parent.as_deref()),
        (Some("0001"), Some(orchestrator.as_str()))
    );
    assert!(server.workers.running().is_empty());
}
