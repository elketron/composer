//! A work's stages run as tickets worked by real `composer worker` processes. The work's state
//! is on its `index.md`: a stage is started explicitly (`work.run`), a gate waits for review,
//! and `rerun`/`approved` are page state the board and editor write. The stage worker's prompt
//! is the stage's scoped context, not the usual one.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::collections::VecDeque;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::pipeline::{Pipeline, Work};
use composer_server::worker::ticket::set_field;
use composer_server::{Server, WorkspaceHome, init_workspace};
use serde_json::json;
use support::{Reply, Scripts, fake_model_logged, send, until};

#[tokio::test]
async fn stages_run_in_order_and_stop_at_a_gate() {
    let (base_url, requests) = fake_model_logged(Scripts {
        keyed: vec![
            (
                "You run stage 01_draft",
                VecDeque::from([
                    Reply::Call("write", json!({ "url": "site://flow/note/01_draft/draft.md", "content": "---\ntitle: Draft\ntype: page\n---\n# Draft\nshort\n" })),
                    Reply::Call("finish", json!({ "status": "done", "summary": "drafted" })),
                    Reply::Text("finished"),
                    // Again, after the rules changed.
                    Reply::Call("write", json!({ "url": "site://flow/note/01_draft/draft.md", "content": "---\ntitle: Draft\ntype: page\n---\n# Draft\nbrief\n" })),
                    Reply::Call("finish", json!({ "status": "done", "summary": "drafted to the new rules" })),
                    Reply::Text("finished"),
                ]),
            ),
            (
                "You run stage 02_polish",
                VecDeque::from([
                    Reply::Call("write", json!({ "url": "site://flow/note/02_polish/final.md", "content": "---\ntitle: Final\ntype: page\n---\n# Final\nshorter\n" })),
                    Reply::Call("finish", json!({ "status": "done", "summary": "polished" })),
                    Reply::Text("finished"),
                    // Run again, with the user's note.
                    Reply::Call("write", json!({ "url": "site://flow/note/02_polish/final.md", "content": "---\ntitle: Final\ntype: page\n---\n# Final\nshortest\n" })),
                    Reply::Call("finish", json!({ "status": "done", "summary": "polished again" })),
                    Reply::Text("finished"),
                    // From the new draft.
                    Reply::Call("write", json!({ "url": "site://flow/note/02_polish/final.md", "content": "---\ntitle: Final\ntype: page\n---\n# Final\nbrief, polished\n" })),
                    Reply::Call("finish", json!({ "status": "done", "summary": "polished the new draft" })),
                    Reply::Text("finished"),
                ]),
            ),
            (
                "You run stage 03_publish",
                VecDeque::from([
                    Reply::Call("write", json!({ "url": "site://flow/note/03_publish/out.md", "content": "---\ntitle: Out\ntype: page\n---\n# Out\n" })),
                    Reply::Call("finish", json!({ "status": "done", "summary": "published" })),
                    Reply::Text("finished"),
                ]),
            ),
        ],
        ..Scripts::default()
    })
    .await;

    let dir = tempfile::tempdir().unwrap();
    let home = WorkspaceHome::new(dir.path());
    init_workspace(&home).await.unwrap();
    std::fs::write(
        home.config_file(),
        format!("[provider]\nbase_url = \"{base_url}\"\n"),
    )
    .unwrap();
    let flow = dir.path().join("sites/flow");
    for (path, text) in [
        (
            "index.md",
            "---\ntitle: Flow\ntype: pipeline\n---\n# Flow\n\nDraft, then polish.\n\n## Works\n",
        ),
        (
            "_config/rules.md",
            "---\ntitle: Rules\n---\n# Rules\n\nBe brief, always.\n",
        ),
        (
            "stages/01_draft.md",
            "---\ntitle: Draft\ntype: stage\ngate: false\n---\n# 01 · Draft\n\n## Inputs\n\n| | file | for |\n|---|---|---|\n\
             | working | `brief` | what |\n| reference | `_config/rules.md` | how |\n\n## Process\n\nDraft it.\n\n## Outputs\n\n- `01_draft/draft.md`\n",
        ),
        (
            "stages/03_publish.md",
            "---\ntitle: Publish\ntype: stage\n---\n# 03 · Publish\n\n## Inputs\n\n| | file | for |\n|---|---|---|\n\
             | working | `02_polish/final.md` | the text |\n\n## Outputs\n\n- `03_publish/out.md`\n",
        ),
        (
            "stages/02_polish.md",
            "---\ntitle: Polish\ntype: stage\ngate: true\n---\n# 02 · Polish\n\n## Inputs\n\n| | file | for |\n|---|---|---|\n\
             | working | `01_draft/draft.md` | the draft |\n\n## Process\n\nPolish it.\n\n## Outputs\n\n- `02_polish/final.md`\n",
        ),
    ] {
        std::fs::create_dir_all(flow.join(path).parent().unwrap()).unwrap();
        std::fs::write(flow.join(path), text).unwrap();
    }
    // A git project the work links; its stages only read it.
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
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&project)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
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
        ClientCommand::WorkNew {
            pipeline: "flow".into(),
            name: "note".into(),
            project: Some(project.to_string_lossy().to_string()),
            brief: Some("A note about stages.".into()),
        },
    )
    .await;
    until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;

    // Run it without a session: one is made for it.
    send(
        &mut socket,
        2,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    let reply = until(&mut socket, 30, |e| {
        matches!(
            e,
            ServerMessage::Ok { id: 2, .. } | ServerMessage::Error { id: Some(2), .. }
        )
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Work { work },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    assert_eq!(work.status.as_deref(), Some("01 draft · running"));

    // The first stage runs; nothing starts the next one behind your back.
    until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::StageEnded { stage, status, summary, .. }
            if stage == "01_draft" && status == "done" && summary == "drafted")
    })
    .await;
    // Let the ending's page writes land.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let work = || Work::load(Pipeline::from_dir("flow", &flow).unwrap(), "note").unwrap();
    assert_eq!(work().stage_status("01_draft"), "done");
    assert_eq!(
        work().stage_field("01_draft", "ticket").as_deref(),
        Some("0001")
    );
    assert_eq!(
        work().stage_field("01_draft", "summary").as_deref(),
        Some("drafted")
    );
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(
        ticket.contains("agent: stage")
            && ticket.contains("stage: 01_draft")
            && ticket.contains("status: done"),
        "{ticket}"
    );

    // Run the second explicitly; it ends at its gate.
    send(
        &mut socket,
        3,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 3, .. })
    })
    .await;
    until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::StageEnded { stage, status, summary, .. }
            if stage == "02_polish" && status == "review" && summary == "polished")
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(work().stage_status("02_polish"), "review");
    assert!(flow.join("note/02_polish/final.md").is_file());
    // What the agent wrote is kept beside it, to tell your edits at the gate from it.
    assert_eq!(
        std::fs::read_to_string(flow.join("note/02_polish/.agent/final.md")).unwrap(),
        std::fs::read_to_string(flow.join("note/02_polish/final.md")).unwrap()
    );
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0002.md")).unwrap();
    assert!(
        ticket.contains("agent: stage")
            && ticket.contains("stage: 02_polish")
            && ticket.contains("status: done"),
        "{ticket}"
    );

    // The stage worker's prompt: the stage's context, not the usual one.
    let system = |request: &serde_json::Value| {
        request["messages"][0]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let prompts: Vec<String> = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| {
            r["tools"]
                .as_array()
                .is_some_and(|t| t.iter().any(|t| t["function"]["name"] == "finish"))
        })
        .map(system)
        .collect();
    let first = &prompts[0];
    assert!(
        first.contains("You run stage 01_draft (Draft) of the work `note`"),
        "{first}"
    );
    assert!(
        first.contains("role=\"reference\" for=\"how\">\n# Rules\n\nBe brief, always."),
        "{first}"
    );
    assert!(
        first.contains("A note about stages."),
        "the brief, in the work page"
    );
    assert!(
        !first.contains("# Memory")
            && !first.contains("You work inside a **web of markdown pages**"),
        "{first}"
    );
    // No worktree for stages that only read the project; the prompt says so.
    assert!(!dir.path().join("state/worktrees/0001").exists());
    assert!(
        first.contains("- the work is about the project at "),
        "{first}"
    );
    let second = prompts
        .iter()
        .find(|p| p.contains("stage 02_polish"))
        .unwrap();
    assert!(
        second.contains("role=\"working\" for=\"the draft\">\n# Draft\nshort"),
        "{second}"
    );

    // The next stage waits for review.
    send(
        &mut socket,
        4,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Error { id: Some(4), .. })
    })
    .await;
    let Some(ServerMessage::Error { message, .. }) = reply.last() else {
        panic!()
    };
    assert!(message.contains("02_polish waits for review"), "{message}");

    // At the gate, as an editor writes it (the board does the same): the work's index.
    let gate = |status: &str, note: &str| {
        let path = flow.join("note/index.md");
        let raw = std::fs::read_to_string(&path).unwrap();
        let raw = set_field(&raw, "stage", "02_polish");
        let raw = set_field(&raw, "status", status);
        let raw = if status == "approved" {
            set_field(&raw, "done", "[\"01_draft\",\"02_polish\"]")
        } else {
            raw
        };
        let raw = if note.is_empty() {
            raw
        } else {
            set_field(&raw, "note", &format!("{{\"02_polish\":\"{note}\"}}"))
        };
        std::fs::write(&path, raw).unwrap();
    };
    let ended = |stage: &'static str, status: &'static str| move |e: &ServerMessage| matches!(e, ServerMessage::StageEnded { stage: s, status: t, .. } if s == stage && t == status);
    // A gate is data; the stage runs again when asked.
    gate("rerun", "Make it shorter.");
    send(
        &mut socket,
        5,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 5, .. })
    })
    .await;
    until(&mut socket, 60, ended("02_polish", "review")).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let rerun =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0003.md")).unwrap();
    assert!(
        rerun.contains("stage: 02_polish") && rerun.contains("note: \"Make it shorter.\""),
        "{rerun}"
    );
    let again = requests
        .lock()
        .unwrap()
        .iter()
        .rev()
        .map(system)
        .find(|p| p.contains("stage 02_polish"))
        .unwrap();
    assert!(
        again.contains("Their note:\n\n> Make it shorter."),
        "{again}"
    );
    assert_eq!(
        work().stage_field("02_polish", "summary").as_deref(),
        Some("polished again")
    );

    // Continue: approving is data; the third stage runs when asked, and finishes the work.
    gate("approved", "");
    send(
        &mut socket,
        6,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 6, .. })
    })
    .await;
    until(&mut socket, 60, ended("03_publish", "done")).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(work().done().iter().any(|s| s == "02_polish"));
    assert_eq!(work().stage_status("03_publish"), "done");
    assert!(flow.join("note/03_publish/out.md").is_file());
    // Nothing watches the page: no edit starts a stage on its own.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        !dir.path()
            .join("sites/orchestration/tickets/0005.md")
            .exists()
    );

    // Stale: a rule the draft read changes. Run reruns from there.
    let index = std::fs::read_to_string(flow.join("note/index.md")).unwrap();
    assert!(
        index.contains("inputs: {\"01_draft\":{\"site://flow/_config/rules.md\":"),
        "{index}"
    );
    std::fs::write(
        flow.join("_config/rules.md"),
        "---\ntitle: Rules\n---\n# Rules\n\nBe briefer.\n",
    )
    .unwrap();
    send(&mut socket, 7, ClientCommand::PipelineList).await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 7, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Pipelines { works, .. },
        ..
    }) = reply.last()
    else {
        panic!()
    };
    let stale: Vec<bool> = works[0].stages.iter().map(|s| s.stale).collect();
    assert_eq!(stale, [true, true, true]);
    assert_eq!(works[0].status.as_deref(), Some("01 draft · stale"));
    send(
        &mut socket,
        8,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 8, .. })
    })
    .await;
    until(&mut socket, 60, ended("01_draft", "done")).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(
        work().stage_field("01_draft", "summary").as_deref(),
        Some("drafted to the new rules")
    );
    send(
        &mut socket,
        9,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 9, .. })
    })
    .await;
    until(&mut socket, 60, ended("02_polish", "review")).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        std::fs::read_to_string(flow.join("note/02_polish/final.md"))
            .unwrap()
            .contains("brief, polished")
    );
    send(&mut socket, 10, ClientCommand::PipelineList).await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 10, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Pipelines { works, .. },
        ..
    }) = reply.last()
    else {
        panic!()
    };
    let stale: Vec<bool> = works[0].stages.iter().map(|s| s.stale).collect();
    // 03 read the old polish: stale until 02 is approved and run again.
    assert_eq!(stale, [false, false, true]);
}
