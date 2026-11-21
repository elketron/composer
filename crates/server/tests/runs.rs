//! `run(kind)` from Rust, not stored: a single shot is one model call without tools; a workflow
//! run has the read-only tools and ends with `finish(value)`, whose value is its result. Neither
//! needs a session or a session folder.

mod support;

use composer_protocol::{RunKind, ServerMessage};
use composer_server::agent::RunSettings;
use composer_server::provider::ModelEvent;
use composer_server::session::run_once;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

/// Nothing stored: no session, no session folder.
fn nothing_stored(dir: &tempfile::TempDir, server: &composer_server::Server) {
    assert!(server.sessions.list().is_empty());
    let sessions = dir.path().join("state/sessions");
    let stored = std::fs::read_dir(&sessions)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(stored, 0, "{}", sessions.display());
}

#[tokio::test]
async fn a_single_shot_is_one_call_without_tools() {
    let (dir, server, provider, _addr) = start(vec![vec![
        ModelEvent::Content("four".into()),
        finished(vec![]),
    ]])
    .await;
    let settings = RunSettings::new(RunKind::SingleShot);
    let (result, sink) = run_once(
        &server,
        &settings,
        "two and two?",
        None,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(result.unwrap(), json!("four"));
    assert_eq!(sink.result(), Some(json!("four")));
    let requests = provider.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].tools.is_empty());
    assert!(
        sink.events().iter().any(|e| matches!(
            e,
            ServerMessage::MessageDone { message, .. } if message.content == "four"
        )),
        "{:?}",
        sink.events()
    );
    nothing_stored(&dir, &server);
}

#[tokio::test]
async fn a_single_shot_ends_after_its_call_whatever_the_model_asks() {
    let (_dir, server, provider, _addr) = start(vec![vec![
        ModelEvent::Content("let me look".into()),
        finished(vec![tool_call(
            "c1",
            "open",
            json!({ "url": "site://memory" }),
        )]),
    ]])
    .await;
    let settings = RunSettings::new(RunKind::SingleShot);
    let (result, sink) =
        run_once(&server, &settings, "look", None, &CancellationToken::new()).await;
    assert_eq!(result.unwrap(), json!("let me look"));
    assert_eq!(provider.requests().len(), 1);
    assert!(
        !sink
            .events()
            .iter()
            .any(|e| matches!(e, ServerMessage::ToolCallStarted { .. })),
        "no tool ran"
    );
}

#[tokio::test]
async fn a_workflow_run_reads_and_returns_its_finish_value() {
    let (dir, server, provider, _addr) = start(vec![
        vec![finished(vec![tool_call(
            "c1",
            "open",
            json!({ "url": "site://memory" }),
        )])],
        vec![finished(vec![tool_call(
            "c2",
            "write",
            json!({ "url": "site://memory/x.md", "content": "x" }),
        )])],
        vec![finished(vec![tool_call(
            "c3",
            "finish",
            json!({ "value": { "pages": 3 } }),
        )])],
        vec![ModelEvent::Content("never asked".into()), finished(vec![])],
    ])
    .await;
    let settings = RunSettings::new(RunKind::Workflow);
    let (result, sink) = run_once(
        &server,
        &settings,
        "count the pages",
        None,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(result.unwrap(), json!({ "pages": 3 }));
    assert_eq!(sink.result(), Some(json!({ "pages": 3 })));

    // The read-only tools, exactly.
    let requests = provider.requests();
    assert_eq!(requests.len(), 3, "finish ends the run");
    let mut offered: Vec<&str> = requests[0].tools.iter().map(|t| t.name.as_str()).collect();
    offered.sort();
    assert_eq!(
        offered,
        ["finish", "list_sites", "open", "search", "set_site"]
    );

    // It read; it could not write.
    let results: Vec<(String, bool)> = sink
        .events()
        .into_iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result, is_error)),
            _ => None,
        })
        .collect();
    assert!(!results[0].1, "{results:?}");
    assert!(
        results[1].1 && results[1].0.contains("unknown tool `write`"),
        "{results:?}"
    );
    assert!(!dir.path().join("sites/memory/x.md").exists());
    nothing_stored(&dir, &server);
}

#[tokio::test]
async fn a_workflow_run_that_stops_returns_its_answer() {
    let (_dir, server, _provider, _addr) = start(vec![vec![
        ModelEvent::Content("nothing to do".into()),
        finished(vec![]),
    ]])
    .await;
    let settings = RunSettings::new(RunKind::Workflow);
    let (result, _sink) = run_once(&server, &settings, "do", None, &CancellationToken::new()).await;
    assert_eq!(result.unwrap(), json!("nothing to do"));
}

#[tokio::test]
async fn a_run_in_a_site_can_change_its_site_without_a_session() {
    let (dir, server, provider, _addr) = start(vec![
        vec![finished(vec![tool_call(
            "c1",
            "set_site",
            json!({ "site": "plans" }),
        )])],
        vec![finished(vec![tool_call(
            "c2",
            "finish",
            json!({ "value": "moved" }),
        )])],
    ])
    .await;
    let settings = RunSettings::new(RunKind::Workflow);
    let (result, sink) = run_once(
        &server,
        &settings,
        "go to plans",
        Some("memory".into()),
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(result.unwrap(), json!("moved"));
    use composer_server::session::RunSink;
    assert_eq!(sink.site().as_deref(), Some("plans"));
    // The prompt of each step has the site it was in.
    let system = |i: usize| match &provider.requests()[i].messages[0] {
        composer_server::provider::ModelMessage::System { content } => content.clone(),
        other => panic!("{other:?}"),
    };
    assert!(system(0).contains("site://memory"), "{}", system(0));
    assert!(
        system(1).contains("You are in `site://plans`"),
        "{}",
        system(1)
    );
    nothing_stored(&dir, &server);
}

#[tokio::test]
async fn a_session_runs_its_kind() {
    use composer_protocol::{ClientCommand, CommandResult};
    let (_dir, server, provider, addr) = start(vec![vec![finished(vec![tool_call(
        "c1",
        "finish",
        json!({ "value": "done" }),
    )])]])
    .await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            kind: RunKind::Workflow,
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
        panic!("{reply:?}")
    };
    assert_eq!(session.kind, RunKind::Workflow);
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: session.id.clone(),
            text: "go".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(events.iter().any(|e| matches!(
        e,
        ServerMessage::ToolResult { result, is_error: false, .. } if result == "finished"
    )));
    let mut offered: Vec<String> = provider.requests()[0]
        .tools
        .iter()
        .map(|t| t.name.clone())
        .collect();
    offered.sort();
    assert_eq!(
        offered,
        ["finish", "list_sites", "open", "search", "set_site"]
    );
}
