//! Workers a restart cut off: the last server was killed while they ran, so their sessions
//! still say `running`. The next one pauses those that can resume and fails the rest; Stop then
//! fails a paused one for good.

mod support;

use composer_protocol::{SessionInfo, TurnState, WorkerState};
use composer_server::Config;
use support::*;

fn ticket(dir: &std::path::Path, id: &str, harness_session: &str) {
    std::fs::create_dir_all(dir.join("sites/orchestration/tickets")).unwrap();
    std::fs::write(
        dir.join(format!("sites/orchestration/tickets/{id}.md")),
        format!(
            "---\ntitle: Ticket {id}\ntype: ticket\nid: \"{id}\"\nstatus: claimed\nagent: worker\nharness_session: \"{harness_session}\"\n---\n# Ticket {id}\n"
        ),
    )
    .unwrap();
}

fn ticket_text(dir: &std::path::Path, id: &str) -> String {
    std::fs::read_to_string(dir.join(format!("sites/orchestration/tickets/{id}.md"))).unwrap()
}

#[tokio::test]
async fn a_restart_pauses_or_fails_cut_off_workers() {
    let (dir, server, _provider, _addr) = start_with(Config::defaults(), vec![]).await;
    let root = dir.path();
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    ticket(root, "0001", "claude-session-1");
    ticket(root, "0002", "");
    let worker = |id: &str, ticket: &str, state: WorkerState| SessionInfo {
        id: id.into(),
        agent: "worker".into(),
        project: None,
        ticket: Some(ticket.into()),
        parent: Some(orchestrator.id().into()),
        worker: Some(state),
        state: TurnState::Running,
        last_seq: 0,
        context_tokens: 0,
        model: None,
        preview: None,
        updated: String::new(),
    };
    server
        .sessions
        .create_with_info(worker("w1", "0001", WorkerState::Running))
        .unwrap();
    server
        .sessions
        .create_with_info(worker("w2", "0002", WorkerState::Starting))
        .unwrap();

    server.workers.recover_orphans(&server);

    // Claude Code can resume: paused, as after a clean shutdown.
    let w1 = server.sessions.get("w1").unwrap().info();
    assert_eq!(w1.worker, Some(WorkerState::Paused));
    assert_eq!(w1.state, TurnState::Idle);
    let text = ticket_text(root, "0001");
    assert!(text.contains("status: paused"), "{text}");
    assert!(text.contains("paused_reason: shutdown"), "{text}");
    // Nothing to resume: failed.
    assert_eq!(
        server.sessions.get("w2").unwrap().info().worker,
        Some(WorkerState::Failed)
    );
    assert!(ticket_text(root, "0002").contains("status: failed"));

    // Stop on the paused one: it won't go on.
    assert!(server.workers.stop_paused(&server, "w1"));
    assert_eq!(
        server.sessions.get("w1").unwrap().info().worker,
        Some(WorkerState::Failed)
    );
    assert!(ticket_text(root, "0001").contains("status: failed"));
    // Not a paused worker (any more): Stop does what it always did.
    assert!(!server.workers.stop_paused(&server, "w1"));
    assert!(!server.workers.stop_paused(&server, orchestrator.id()));
}
