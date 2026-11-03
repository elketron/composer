//! A ticket's diff view: the structural diff from difft when it is on the PATH (and the text
//! diff alone when it isn't), and review comments written onto the ticket page.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use composer_core::SiteUrl;
use composer_protocol::http::{DiffComment, ProjectDiff};
use composer_protocol::{ChangedBy, ClientCommand, CommandResult, ServerMessage};
use support::*;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
}

fn on_path(tool: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(tool))
        .find(|p| p.is_file())
}

async fn changes(socket: &mut Socket, id: u64) -> ProjectDiff {
    send(
        socket,
        id,
        ClientCommand::TicketChanges { ticket: "1".into() },
    )
    .await;
    let reply = until(
        socket,
        |e| matches!(e, ServerMessage::Ok { id: i, .. } if *i == id),
    )
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Changes { changes },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    changes.clone()
}

async fn comment(socket: &mut Socket, id: u64, comments: Vec<DiffComment>) -> Vec<String> {
    let comments_empty = comments.is_empty();
    send(
        socket,
        id,
        ClientCommand::TicketComment {
            ticket: "1".into(),
            comments,
        },
    )
    .await;
    let reply = until(
        socket,
        |e| matches!(e, ServerMessage::Ok { id: i, .. } if *i == id),
    )
    .await;
    if !comments_empty {
        assert!(
            reply.iter().any(|m| matches!(
                m,
                ServerMessage::PageChanged {
                    by: ChangedBy::App,
                    ..
                }
            )),
            "a user's comments are written as the app: {reply:?}"
        );
    }
    let Some(ServerMessage::Ok {
        result: CommandResult::Comments { lines },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    lines.clone()
}

#[tokio::test]
async fn semantic_diff_falls_back_without_difft_and_comments_land_on_the_ticket() {
    let difft = on_path("difft");
    let git_bin = on_path("git").expect("git");
    let (dir, server, _provider, addr) = start(vec![]).await;
    let home = dir.path();
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("a.rs"), "fn a() {\n    let x = 1;\n}\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["add", "."],
        &["commit", "-qm", "init"],
    ] {
        git(&project, args);
    }
    let worktree = home.join("state/worktrees/0001");
    git(
        &project,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "ticket/0001",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("a.rs"), "fn a() {\n    let x = 2;\n}\n").unwrap();
    std::fs::write(worktree.join("notes.txt"), "plain\n").unwrap();
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "-qm", "work"]);
    let url = SiteUrl::new("orchestration", "tickets/0001.md").unwrap();
    server
        .write_page(
            &url,
            "---\ntitle: T\ntype: ticket\nstatus: done\nbranch: ticket/0001\n---\n# T\n\n## Result\n\ndone\n",
            ChangedBy::Agent,
        )
        .unwrap();
    let mut socket = connect(addr, &server.token).await;

    // PATH holds git only: no difft, so every file has just the text diff.
    let bin = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&git_bin, bin.path().join("git")).unwrap();
    let old_path = std::env::var_os("PATH").unwrap();
    unsafe { std::env::set_var("PATH", bin.path()) };
    let plain = changes(&mut socket, 1).await;
    assert!(
        plain.files.iter().all(|f| f.semantic.is_none()),
        "{plain:?}"
    );
    assert!(plain.diff.contains("+    let x = 2;"));

    // With difft on the PATH the Rust file has a structural diff; the plain text file (which
    // difft would only diff as text) does not. The text diff stays alongside.
    if let Some(difft) = difft {
        std::os::unix::fs::symlink(difft, bin.path().join("difft")).unwrap();
        let rich = changes(&mut socket, 2).await;
        let rs = rich.files.iter().find(|f| f.path == "a.rs").unwrap();
        let semantic = rs.semantic.as_deref().expect("a structural diff");
        assert!(
            semantic.contains("Rust") && semantic.contains("let x = 2;"),
            "{semantic}"
        );
        assert!(
            rich.files
                .iter()
                .find(|f| f.path == "notes.txt")
                .unwrap()
                .semantic
                .is_none()
        );
        assert!(rich.diff.contains("+    let x = 2;"));
    } else {
        eprintln!("difft is not installed: skipping the semantic leg");
    }
    unsafe { std::env::set_var("PATH", old_path) };

    // Comments: appended under a new `## Comments` heading, then to it; an empty list reads.
    let one = DiffComment {
        file: "a.rs".into(),
        start: 2,
        end: 2,
        message: "why 2?".into(),
        quote: Some("let x = 2;".into()),
    };
    let lines = comment(&mut socket, 3, vec![one]).await;
    assert_eq!(lines, ["- a.rs:2:2 — `let x = 2;`: why 2?"]);
    let two = DiffComment {
        file: "notes.txt".into(),
        start: 1,
        end: 3,
        message: "drop this".into(),
        quote: Some("a".repeat(200)),
    };
    let lines = comment(&mut socket, 4, vec![two]).await;
    assert_eq!(
        lines,
        [
            "- a.rs:2:2 — `let x = 2;`: why 2?",
            "- notes.txt:1:3 — drop this"
        ]
    );
    assert_eq!(comment(&mut socket, 5, vec![]).await, lines);
    let page = std::fs::read_to_string(home.join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(
        page.contains("## Result\n\ndone\n\n## Comments\n\n- a.rs:2:2"),
        "{page}"
    );
}
