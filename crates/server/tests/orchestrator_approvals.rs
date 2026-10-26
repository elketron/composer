//! A worker's approval its orchestrator decides: a turn that ends without `answer_approval`
//! gets one reminder; it decides then, or the user is asked.

mod support;

use std::time::Duration;

use composer_protocol::{ApprovalDecision, ApprovalKind, SessionInfo, TurnState};
use composer_server::Config;
use composer_server::provider::{ModelEvent, ModelMessage};
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn worker_of(parent: &str, ticket: &str) -> SessionInfo {
    SessionInfo {
        id: format!("w{ticket}"),
        agent: "worker".into(),
        project: None,
        ticket: Some(ticket.into()),
        parent: Some(parent.into()),
        worker: None,
        state: TurnState::Running,
        last_seq: 0,
        context_tokens: 0,
        model: None,
        preview: None,
        updated: String::new(),
    }
}

#[tokio::test]
async fn a_forgotten_answer_gets_a_reminder_then_goes_to_the_user() {
    let text = |t: &str| vec![ModelEvent::Content(t.into()), finished(vec![])];
    let approve = vec![finished(vec![tool_call(
        "c1",
        "answer_approval",
        json!({ "ticket": "0001", "decision": "approve", "reason": "runs its tests" }),
    )])];
    // 0001: forgets, is reminded, approves. 0002: forgets twice.
    let scripts = vec![
        text("Looks fine."),
        approve,
        text("ok"),
        text("Fine."),
        text("still fine"),
    ];
    let (_dir, server, provider, _addr) = start_with(Config::defaults(), scripts).await;
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    let cancel = CancellationToken::new();
    let ask = |worker| {
        server.ask_approval(
            worker,
            ApprovalKind::Tool,
            "Bash: cargo test".into(),
            "cargo test".into(),
            None,
            &cancel,
        )
    };

    let first = server
        .sessions
        .create_with_info(worker_of(orchestrator.id(), "0001"))
        .unwrap();
    let answer = tokio::time::timeout(Duration::from_secs(5), ask(&first))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(answer.decision, ApprovalDecision::Approve);
    assert_eq!(answer.reason.as_deref(), Some("runs its tests"));
    let reminded = provider.requests()[1]
        .messages
        .iter()
        .rev()
        .find_map(|m| match m {
            ModelMessage::User { content, .. } => Some(content.clone()),
            _ => None,
        });
    let reminded = reminded.unwrap();
    assert!(
        reminded.starts_with("[worker #w0001 on ticket 0001 asks to approve: Bash: cargo test]"),
        "{reminded}"
    );
    assert!(
        reminded.contains("You ended your turn without deciding this"),
        "{reminded}"
    );

    // Let the turn after the answer end before the next one.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let second = server
        .sessions
        .create_with_info(worker_of(orchestrator.id(), "0002"))
        .unwrap();
    let asked = tokio::spawn({
        let server = server.clone();
        async move {
            server
                .ask_approval(
                    &second,
                    ApprovalKind::Tool,
                    "Bash: git push".into(),
                    "git push".into(),
                    None,
                    &CancellationToken::new(),
                )
                .await
        }
    });
    let body = loop {
        if let Some(composer_protocol::ServerMessage::ApprovalRequest { body, .. }) =
            server.pending_approvals.all().pop()
        {
            break body;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert!(
        body.starts_with("Its orchestrator asks you: it ended its turn without deciding"),
        "{body}"
    );
    assert_eq!(provider.requests().len(), 5, "a reminder, not a third try");
    asked.abort();
}
