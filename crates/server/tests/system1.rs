//! `[system1]`: a fast model decides workers' approvals in one call; one that answers nonsense
//! leaves it to the orchestrator or, as here without one that decides, the user.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::routing::post;
use composer_protocol::{
    ApprovalDecision, ApprovalKind, Role, ServerMessage, SessionInfo, TurnState,
};
use composer_server::Config;
use serde_json::{Value, json};
use support::*;
use tokio_util::sync::CancellationToken;

/// What it answers, and the requests it got.
type Fake = (Arc<Mutex<String>>, Arc<Mutex<Vec<Value>>>);

/// An OpenAI-compatible endpoint that streams `answer` and keeps what it was asked.
async fn fake_system1(answer: Arc<Mutex<String>>, asked: Arc<Mutex<Vec<Value>>>) -> String {
    async fn complete(State((answer, asked)): State<Fake>, body: axum::Json<Value>) -> String {
        asked.lock().unwrap().push(body.0);
        let chunk = json!({ "choices": [{ "delta": { "content": *answer.lock().unwrap() }, "finish_reason": "stop" }] });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }
    let app = Router::new()
        .route("/v1/chat/completions", post(complete))
        .with_state((answer, asked));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn system1_decides_workers_approvals() {
    let answer = Arc::new(Mutex::new(String::new()));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut config = Config::defaults();
    config.system1.model = "tiny".into();
    config.system1.base_url = fake_system1(answer.clone(), asked.clone()).await;
    let (_dir, server, _provider, _addr) = start_with(config, vec![]).await;
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    let worker = server
        .sessions
        .create_with_info(SessionInfo {
            id: "w1".into(),
            agent: "worker".into(),
            project: Some("/home/me/game".into()),
            ticket: Some("0001".into()),
            parent: Some(orchestrator.id().into()),
            worker: None,
            state: TurnState::Running,
            last_seq: 0,
            context_tokens: 0,
            model: None,
            preview: None,
            updated: String::new(),
        })
        .unwrap();
    let cancel = CancellationToken::new();
    let ask = || {
        server.ask_approval(
            &worker,
            ApprovalKind::Tool,
            "Bash: cargo test".into(),
            "cargo test --workspace".into(),
            None,
            &cancel,
        )
    };

    *answer.lock().unwrap() = r#"{"decision": "approve", "reason": "runs its tests"}"#.into();
    let approved = ask().await.unwrap();
    assert_eq!(approved.decision, ApprovalDecision::Approve);
    assert_eq!(approved.reason.as_deref(), Some("runs its tests"));
    let request = asked.lock().unwrap()[0].clone();
    assert_eq!(request["model"], "tiny");
    let question = request["messages"][1]["content"].as_str().unwrap();
    assert!(
        question.contains("Bash: cargo test") && question.contains("/home/me/game"),
        "{question}"
    );
    // Shown in the worker's chat.
    let (events, _) = worker.transcript().history(None, 10).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        ServerMessage::MessageDone { message, .. }
            if message.role == Role::System && message.content == "tiny approved: Bash: cargo test. runs its tests"
    )));

    *answer.lock().unwrap() =
        "```json\n{\"decision\": \"deny\", \"reason\": \"pushes\"}\n```".into();
    assert_eq!(ask().await.unwrap().decision, ApprovalDecision::Deny);

    // Nonsense: no orchestrator that decides, so it is the user's (and waits for them).
    *answer.lock().unwrap() = "hmm".into();
    assert!(
        tokio::time::timeout(Duration::from_millis(500), ask())
            .await
            .is_err()
    );
    // Not a worker: never system1's.
    let asked_before = asked.lock().unwrap().len();
    let plain = server.sessions.create("default", None).unwrap();
    let not_a_worker = server.ask_approval(
        &plain,
        ApprovalKind::Tool,
        "Bash: ls".into(),
        "ls".into(),
        None,
        &cancel,
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), not_a_worker)
            .await
            .is_err()
    );
    assert_eq!(asked.lock().unwrap().len(), asked_before);
}
