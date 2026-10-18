//! An orchestrator spawns a real worker process (this crate's `composer` binary) on a
//! ticket; both talk to a fake OpenAI-compatible model that answers by role.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use composer_protocol::{
    ClientCommand, ClientRequest, CommandResult, MessageSource, Role, ServerMessage, WorkerState,
};
use composer_server::{Home, Server, init_home};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// A scripted reply: text, or one tool call.
#[derive(Clone)]
enum Reply {
    Text(&'static str),
    Call(&'static str, Value),
}

#[derive(Default)]
struct Scripts {
    orchestrator: VecDeque<Reply>,
    worker: VecDeque<Reply>,
}

/// The role is told apart by the tools offered: only orchestrators get `spawn_worker`,
/// only workers `finish`.
async fn completions(
    State(scripts): State<Arc<Mutex<Scripts>>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let tools: Vec<String> = body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["function"]["name"].as_str().map(str::to_string))
        .collect();
    let reply = {
        let mut scripts = scripts.lock().unwrap();
        let queue = if tools.iter().any(|t| t == "finish") {
            &mut scripts.worker
        } else if tools.iter().any(|t| t == "spawn_worker") {
            &mut scripts.orchestrator
        } else {
            return sse(Reply::Text("(no script)"));
        };
        queue
            .pop_front()
            .unwrap_or(Reply::Text("(script exhausted)"))
    };
    sse(reply)
}

fn sse(reply: Reply) -> axum::response::Response {
    let chunk = |delta: Value, finish: Option<&str>| {
        format!(
            "data: {}\n\n",
            json!({ "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }] })
        )
    };
    let body = match reply {
        Reply::Text(text) => chunk(json!({ "content": text }), Some("stop")),
        Reply::Call(name, args) => chunk(
            json!({ "tool_calls": [{ "index": 0, "id": "call_1", "type": "function",
                "function": { "name": name, "arguments": args.to_string() } }] }),
            Some("tool_calls"),
        ),
    } + "data: [DONE]\n\n";
    ([("content-type", "text/event-stream")], body).into_response()
}

async fn fake_model(scripts: Scripts) -> String {
    let app = axum::Router::new()
        .route(
            "/v1/models",
            get(|| async { Json(json!({ "data": [{ "id": "fake" }] })) }),
        )
        .route("/v1/chat/completions", post(completions))
        .with_state(Arc::new(Mutex::new(scripts)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}/v1")
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(socket: &mut Socket, id: u64, command: ClientCommand) {
    let json = serde_json::to_string(&ClientRequest { id, command }).unwrap();
    socket.send(Message::Text(json.into())).await.unwrap();
}

async fn until(
    socket: &mut Socket,
    secs: u64,
    stop: impl Fn(&ServerMessage) -> bool,
) -> Vec<ServerMessage> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let frame = tokio::time::timeout_at(deadline, socket.next())
            .await
            .unwrap_or_else(|_| panic!("timed out; got: {events:#?}"))
            .unwrap()
            .unwrap();
        let Message::Text(text) = frame else { continue };
        let event: ServerMessage = serde_json::from_str(&text).unwrap();
        let done = stop(&event);
        events.push(event);
        if done {
            return events;
        }
    }
}

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
            Reply::Call("spawn_worker", json!({ "ticket": "1" })),
            Reply::Text("worker started"),
            // after the worker's finish message:
            Reply::Text("ticket 0001 is done"),
        ]),
        worker: VecDeque::from([
            Reply::Call("write", json!({ "url": "site://memory/topics/note", "content": note })),
            Reply::Call("finish", json!({ "status": "done", "summary": "wrote the note" })),
            Reply::Text("finished"),
        ]),
    })
    .await;

    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
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
