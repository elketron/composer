//! A finished ticket's branch: reviewed, merged by the orchestrator once the user approves (and
//! not when they don't), and discarded.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::path::Path;
use std::process::Command;

use composer_core::SiteUrl;
use composer_protocol::{
    ApprovalDecision, ApprovalKind, ChangedBy, ClientCommand, CommandResult, ServerMessage,
};
use composer_server::provider::ModelEvent;
use serde_json::json;
use support::*;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A project with one commit, and ticket `id` done on its own worktree with one commit and one
/// file left uncommitted.
fn ticket_with_work(
    server: &composer_server::Server,
    home: &Path,
    project: &Path,
    id: &str,
    front_extra: &str,
) {
    let worktree = home.join("state/worktrees").join(id);
    git(
        project,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("ticket/{id}"),
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join(format!("{id}.txt")), "work\n").unwrap();
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "-qm", &format!("work for {id}")]);
    std::fs::write(worktree.join(format!("{id}-notes.txt")), "left over\n").unwrap();
    let url = SiteUrl::new("orchestration", &format!("tickets/{id}.md")).unwrap();
    server
        .write_page(
            &url,
            &format!(
                "---\ntitle: Ticket {id}\ntype: ticket\nstatus: done\nbranch: ticket/{id}{front_extra}\n---\n# Ticket {id}\n"
            ),
            ChangedBy::Agent,
        )
        .unwrap();
}

fn ticket_page(home: &Path, id: &str) -> String {
    std::fs::read_to_string(home.join(format!("sites/orchestration/tickets/{id}.md"))).unwrap()
}

#[tokio::test]
async fn review_merge_and_discard() {
    let merge = |id: &str| {
        vec![finished(vec![tool_call(
            id,
            "merge_ticket",
            json!({ "ticket": "1" }),
        )])]
    };
    let say = |text: &str| vec![ModelEvent::Content(text.into()), finished(vec![])];
    // Each: the call (it returns at once), the turn's end, then a turn for the outcome.
    let (dir, server, _provider, addr) = start(vec![
        merge("m1"),
        say("Asked."),
        say("Not merged."),
        merge("m2"),
        say("Asked."),
        say("Merged."),
    ])
    .await;
    let home = dir.path();
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("README"), "hi\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["add", "."],
        &["commit", "-qm", "init"],
    ] {
        git(&project, args);
    }
    ticket_with_work(&server, home, &project, "0001", "");
    ticket_with_work(&server, home, &project, "0002", "");

    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::TicketChanges { ticket: "1".into() },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Changes { changes },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    let review = changes.ticket.as_ref().expect("a ticket review");
    assert_eq!(
        (review.branch.as_str(), review.target.as_deref()),
        ("ticket/0001", Some("main"))
    );
    assert_eq!(review.commits.len(), 1);
    assert!(review.uncommitted && review.conflicts.is_empty());
    let mut files: Vec<&str> = changes.files.iter().map(|f| f.path.as_str()).collect();
    files.sort();
    assert_eq!(files, ["0001-notes.txt", "0001.txt"]);

    // The orchestrator asks; the user says no, then yes.
    send(
        &mut socket,
        2,
        ClientCommand::SessionCreate {
            agent: "orchestrator".into(),
            project: None,
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!()
    };
    let session = session.id.clone();
    send(
        &mut socket,
        3,
        ClientCommand::SessionAttach {
            session: session.clone(),
            since: None,
        },
    )
    .await;
    for (n, decision) in [(4, ApprovalDecision::Deny), (6, ApprovalDecision::Approve)] {
        send(
            &mut socket,
            n,
            ClientCommand::MessageSend {
                session: session.clone(),
                text: "merge ticket 1".into(),
                pages: vec![],
                images: vec![],
                attachments: vec![],
            },
        )
        .await;
        // The call returns at once and the turn ends; the user is asked meanwhile.
        let mut events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
        let asked = events.iter().find_map(|e| match e {
            ServerMessage::ToolResult { result, .. } => Some(result.clone()),
            _ => None,
        });
        assert!(
            asked
                .as_deref()
                .is_some_and(|r| r.starts_with("asked the user to merge ticket 0001 into main")),
            "{asked:?}"
        );
        if !events
            .iter()
            .any(|e| matches!(e, ServerMessage::ApprovalRequest { .. }))
        {
            events = until(&mut socket, |e| {
                matches!(e, ServerMessage::ApprovalRequest { .. })
            })
            .await;
        }
        let events: Vec<_> = events
            .into_iter()
            .filter(|e| matches!(e, ServerMessage::ApprovalRequest { .. }))
            .collect();
        let Some(ServerMessage::ApprovalRequest {
            request,
            kind,
            title,
            body,
            ..
        }) = events.last()
        else {
            panic!()
        };
        assert_eq!(*kind, ApprovalKind::Merge);
        assert_eq!(title, "Merge ticket 0001 into main");
        assert!(
            body.contains("0001.txt") && body.contains("work for 0001"),
            "{body}"
        );
        assert!(body.contains("uncommitted"), "{body}");
        send(
            &mut socket,
            n + 1,
            ClientCommand::ApprovalRespond {
                request: request.clone(),
                decision,
            },
        )
        .await;
        // The outcome comes back as a message, which starts a turn.
        let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
        let told = events
            .iter()
            .find_map(|e| match e {
                ServerMessage::MessageDone { message, .. }
                    if message.content.starts_with("[merge of ticket 0001") =>
                {
                    Some(message.content.clone())
                }
                _ => None,
            })
            .unwrap();
        if decision == ApprovalDecision::Deny {
            assert_eq!(
                told,
                "[merge of ticket 0001: the user declined; it is not merged]"
            );
            assert!(!project.join("0001.txt").exists());
        } else {
            assert!(
                told.starts_with("[merge of ticket 0001: merged into main as "),
                "{told}"
            );
        }
    }

    assert_eq!(
        git(&project, &["log", "-1", "--format=%s"]),
        "Merge ticket 0001: Ticket 0001"
    );
    assert!(project.join("0001.txt").is_file() && project.join("0001-notes.txt").is_file());
    assert!(!home.join("state/worktrees/0001").exists());
    let commit = git(&project, &["rev-parse", "--short", "HEAD"]);
    assert!(ticket_page(home, "0001").contains(&format!("merged: {commit}")));

    // Gone now: its changes say so.
    send(
        &mut socket,
        8,
        ClientCommand::TicketChanges { ticket: "1".into() },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Error { id: Some(8), .. })
    })
    .await;
    assert!(
        matches!(reply.last(), Some(ServerMessage::Error { message, .. })
        if message.contains("was merged"))
    );

    // The other one is thrown away.
    send(
        &mut socket,
        9,
        ClientCommand::TicketDiscard { ticket: "2".into() },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 9, .. })
    })
    .await;
    assert!(!home.join("state/worktrees/0002").exists());
    assert!(!git(&project, &["branch", "--list", "ticket/0002"]).contains("0002"));
    assert!(!project.join("0002.txt").exists());
    assert!(ticket_page(home, "0002").contains("discarded: true"));
}

/// `merge_ticket` with a target branch: an explicit `into` lands the merge there, the ticket's
/// `merge_into:` frontmatter does too (the arg wins), and an unknown branch errors without
/// merging. The checked-out branch keeps its own path and stays untouched.
#[tokio::test]
async fn merging_into_another_branch() {
    // Script per model call: each merge ask returns at once, then a turn for the outcome.
    let call = |id: &str, args: serde_json::Value| {
        vec![finished(vec![tool_call(
            id,
            "merge_ticket",
            json!({ "ticket": id, "into": args }),
        )])]
    };
    let say = |text: &str| vec![ModelEvent::Content(text.into()), finished(vec![])];
    // into → lands on dev; frontmatter → lands on staging; unknown → error; no into → main.
    let (dir, server, _provider, addr) = start(vec![
        call("1", json!("dev")),
        say("Asked."),
        say("Merged."),
        call("2", json!(null)),
        say("Asked."),
        say("Merged."),
        call("3", json!("ghost")),
        say("Nope."),
        call("4", json!(null)),
        say("Asked."),
        say("Merged."),
        call("5", json!("dev")),
        say("Asked."),
        say("Merged."),
        call("6", json!(null)),
        say("Nope."),
    ])
    .await;
    let home = dir.path();
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("README"), "hi\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["add", "."],
        &["commit", "-qm", "init"],
    ] {
        git(&project, args);
    }
    for b in ["dev", "staging"] {
        git(&project, &["branch", b, "main"]);
    }
    // The main checkout is left dirty: merging elsewhere must not touch it.
    std::fs::write(project.join("dirty.txt"), "untouched\n").unwrap();

    ticket_with_work(&server, home, &project, "0001", "");
    ticket_with_work(&server, home, &project, "0002", "\nmerge_into: staging");
    ticket_with_work(&server, home, &project, "0003", "");
    ticket_with_work(&server, home, &project, "0004", "");
    ticket_with_work(&server, home, &project, "0005", "\nmerge_into: staging");
    ticket_with_work(&server, home, &project, "0006", "");

    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "orchestrator".into(),
            project: None,
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
        panic!()
    };
    let session = session.id.clone();
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: session.clone(),
            since: None,
        },
    )
    .await;

    let mut next = 3u64;
    let mut ask = |id: &str| {
        let n = next;
        next += 2;
        (n, id.to_string())
    };
    async fn approve(
        socket: &mut Socket,
        n: u64,
        session: &str,
        id: &str,
        expect_title: &str,
    ) -> String {
        send(
            socket,
            n,
            ClientCommand::MessageSend {
                session: session.into(),
                text: format!("merge ticket {id}"),
                pages: vec![],
                images: vec![],
                attachments: vec![],
            },
        )
        .await;
        let mut events = until(socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
        let asked = events.iter().find_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result.clone(), *is_error)),
            _ => None,
        });
        if asked.as_ref().is_some_and(|(_, e)| *e) {
            return asked.unwrap().0;
        }
        assert!(
            asked
                .as_ref()
                .is_some_and(|(r, _)| r.contains(expect_title) || r.starts_with("asked")),
            "{asked:?}"
        );
        if !events
            .iter()
            .any(|e| matches!(e, ServerMessage::ApprovalRequest { .. }))
        {
            events = until(socket, |e| {
                matches!(e, ServerMessage::ApprovalRequest { .. })
            })
            .await;
        }
        let request = events
            .iter()
            .find_map(|e| match e {
                ServerMessage::ApprovalRequest { request, title, .. } => {
                    assert_eq!(title, expect_title, "approval title");
                    Some(request.clone())
                }
                _ => None,
            })
            .unwrap();
        send(
            socket,
            n + 1,
            ClientCommand::ApprovalRespond {
                request,
                decision: ApprovalDecision::Approve,
            },
        )
        .await;
        let events = until(socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
        events
            .iter()
            .find_map(|e| match e {
                ServerMessage::MessageDone { message, .. }
                    if message
                        .content
                        .starts_with(&format!("[merge of ticket {id}")) =>
                {
                    Some(message.content.clone())
                }
                _ => None,
            })
            .unwrap()
    }

    // 1) explicit `into: dev`: the merge lands on dev, main's HEAD unmoved, worktree gone.
    let (n, id) = ask("0001");
    let told = approve(&mut socket, n, &session, &id, "Merge ticket 0001 into dev").await;
    assert!(
        told.starts_with("[merge of ticket 0001: merged into dev as "),
        "{told}"
    );
    let dev_tip = git(&project, &["rev-parse", "--short", "refs/heads/dev"]);
    assert_eq!(
        git(&project, &["log", "-1", "--format=%s", "dev"]),
        "Merge ticket 0001: Ticket 0001"
    );
    assert!(!git(&project, &["branch", "--list", "ticket/0001"]).contains("ticket/0001"));
    assert!(!home.join("state/worktrees/0001").exists());
    assert!(ticket_page(home, "0001").contains(&format!("merged: {dev_tip}")));
    assert!(
        !git(&project, &["worktree", "list"]).contains("composer-merge"),
        "the scratch worktree is gone after the merge"
    );

    assert_eq!(
        git(&project, &["log", "-1", "--format=%s"]),
        "init",
        "main untouched"
    );

    // 2) no arg, ticket declares `merge_into: staging`.
    let (n, id) = ask("0002");
    let told = approve(
        &mut socket,
        n,
        &session,
        &id,
        "Merge ticket 0002 into staging",
    )
    .await;
    assert!(
        told.starts_with("[merge of ticket 0002: merged into staging as "),
        "{told}"
    );
    let staging_tip = git(&project, &["rev-parse", "--short", "refs/heads/staging"]);
    assert!(
        git(&project, &["log", "-1", "--format=%s", "staging"]).starts_with("Merge ticket 0002")
    );
    assert!(ticket_page(home, "0002").contains(&format!("merged: {staging_tip}")));

    // 3) unknown branch: an error, nothing merged.
    let (n, id) = ask("0003");
    let told = approve(
        &mut socket,
        n,
        &session,
        &id,
        "Merge ticket 0003 into ghost",
    )
    .await;
    assert!(
        told.contains("no branch") && told.contains("ghost"),
        "{told}"
    );
    assert!(home.join("state/worktrees/0003").exists());
    assert!(git(&project, &["branch", "--list", "ticket/0003"]).contains("ticket/0003"));

    // 4) no arg, no frontmatter: today's behavior, the checked-out branch.
    let (n, id) = ask("0004");
    let told = approve(&mut socket, n, &session, &id, "Merge ticket 0004 into main").await;
    assert!(
        told.starts_with("[merge of ticket 0004: merged into main as "),
        "{told}"
    );
    assert_eq!(
        git(&project, &["log", "-1", "--format=%s"]),
        "Merge ticket 0004: Ticket 0004"
    );
    assert!(project.join("0004.txt").is_file() && project.join("0004-notes.txt").is_file());

    // 5) the arg wins over the ticket frontmatter: lands on dev, not staging.
    let dev_before = git(&project, &["rev-parse", "--short", "refs/heads/dev"]);
    let (n, id) = ask("0005");
    let told = approve(&mut socket, n, &session, &id, "Merge ticket 0005 into dev").await;
    assert!(
        told.starts_with("[merge of ticket 0005: merged into dev as "),
        "{told}"
    );
    assert_ne!(
        git(&project, &["rev-parse", "--short", "refs/heads/dev"]),
        dev_before,
        "dev advanced"
    );
    assert_eq!(
        git(&project, &["rev-parse", "--short", "refs/heads/staging"]),
        staging_tip,
        "staging untouched: the arg beat the frontmatter"
    );
    assert!(!home.join("state/worktrees/0005").exists());
    assert!(!git(&project, &["branch", "--list", "ticket/0005"]).contains("ticket/0005"));
    assert!(
        !git(&project, &["worktree", "list"]).contains("composer-merge"),
        "no scratch worktree left behind"
    );

    // 6) detached HEAD, no arg and no frontmatter: the tool message, pinned.
    git(&project, &["checkout", "--detach", "--quiet"]);
    let (n, id) = ask("0006");
    let told = approve(&mut socket, n, &session, &id, "unused").await;
    assert_eq!(
        told,
        "no target branch: pass into, add merge_into to the ticket, or check a branch out"
    );
    assert!(home.join("state/worktrees/0006").exists(), "nothing merged");
    assert!(git(&project, &["branch", "--list", "ticket/0006"]).contains("ticket/0006"));

    // The main checkout stayed where it was, dirty file and all.
    assert_eq!(
        std::fs::read_to_string(project.join("dirty.txt")).unwrap(),
        "untouched\n"
    );
}
