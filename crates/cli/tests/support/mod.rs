//! Shared by the worker tests: a fake OpenAI-compatible model that answers by role, and a
//! WebSocket client.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use composer_protocol::{ClientCommand, ClientRequest, ServerMessage};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// A scripted reply: text, or one tool call.
#[derive(Clone)]
pub enum Reply {
    Text(&'static str),
    Call(&'static str, Value),
    /// Never answers (a worker that stays busy).
    Hang,
}

#[derive(Default)]
pub struct Scripts {
    pub orchestrator: VecDeque<Reply>,
    pub worker: VecDeque<Reply>,
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
    if matches!(reply, Reply::Hang) {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
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
        Reply::Hang => unreachable!("answered above"),
        Reply::Call(name, args) => chunk(
            json!({ "tool_calls": [{ "index": 0, "id": "call_1", "type": "function",
                "function": { "name": name, "arguments": args.to_string() } }] }),
            Some("tool_calls"),
        ),
    } + "data: [DONE]\n\n";
    ([("content-type", "text/event-stream")], body).into_response()
}

pub async fn fake_model(scripts: Scripts) -> String {
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

pub type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn send(socket: &mut Socket, id: u64, command: ClientCommand) {
    let json = serde_json::to_string(&ClientRequest { id, command }).unwrap();
    socket.send(Message::Text(json.into())).await.unwrap();
}

pub async fn until(
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

/// A server with an orchestrator whose worker (a real `composer worker` process) is mid-turn,
/// waiting on a model that never answers.
pub struct BusyWorker {
    pub dir: tempfile::TempDir,
    pub server: std::sync::Arc<composer_server::Server>,
    pub addr: std::net::SocketAddr,
    pub socket: Socket,
    pub orchestrator: String,
    pub worker: String,
}

/// Starts it; the orchestrator answers `after` once the worker is done (or cancelled). With
/// `git_project`, the orchestrator works in `<dir>/project`, a git repository with one commit.
/// Needs `just` (the ticket recipe): `None` without it.
pub async fn busy_worker(after: &'static str, git_project: bool) -> Option<BusyWorker> {
    use composer_protocol::{CommandResult, WorkerState};
    use composer_server::{Home, Server, init_home};

    std::process::Command::new("just")
        .arg("--version")
        .output()
        .ok()?;
    let base_url = fake_model(Scripts {
        orchestrator: VecDeque::from([
            Reply::Call("run", json!({ "site": "orchestration", "recipe": "new-ticket", "args": ["Take forever"] })),
            Reply::Call("spawn_worker", json!({ "ticket": "1" })),
            Reply::Text("worker started"),
            Reply::Text(after),
        ]),
        worker: VecDeque::from([Reply::Hang]),
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
    let project = git_project.then(|| {
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("README.md"), "# project\n").unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["add", "."],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "init",
            ],
        ] {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(&project)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        }
        project.to_string_lossy().to_string()
    });
    // SAFETY: set before any server or thread reads it; one test per binary uses this.
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
            project,
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
            text: "start something long".into(),
            pages: vec![],
            images: vec![],
        },
    )
    .await;
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
    let Some(ServerMessage::WorkerStatus { worker, .. }) = events.last() else {
        panic!()
    };
    let worker = worker.clone();
    send(
        &mut socket,
        4,
        ClientCommand::SessionAttach {
            session: worker.clone(),
            since: None,
        },
    )
    .await;
    until(
        &mut socket,
        30,
        |e| matches!(e, ServerMessage::TurnStarted { session, .. } if *session == worker),
    )
    .await;
    Some(BusyWorker {
        dir,
        server,
        addr,
        socket,
        orchestrator,
        worker,
    })
}
