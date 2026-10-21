//! A work's stages run as tickets worked by real `composer worker` processes: the first (no
//! gate) goes on to the second by itself, the second (a gate) waits for review. The stage
//! worker's prompt is the stage's scoped context, not the usual one. At the gate, `rerun` (with
//! a note) runs it again and `approved` starts the third, written into `_stage.md` as the
//! board app does.

mod support;

use std::collections::VecDeque;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage, WorkerState};
use composer_server::{Home, Server, init_home};
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
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
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

    // Both stages run; the second ends at its gate.
    let done = std::cell::Cell::new(0);
    let events = until(&mut socket, 90, |e| {
        if matches!(
            e,
            ServerMessage::WorkerStatus {
                state: WorkerState::Done | WorkerState::Failed,
                ..
            }
        ) {
            done.set(done.get() + 1);
        }
        done.get() == 2
    })
    .await;
    let finished: Vec<(String, WorkerState)> = events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::WorkerStatus { ticket, state, .. }
                if *state != WorkerState::Starting =>
            {
                Some((ticket.clone(), *state))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        finished,
        [
            ("0001".into(), WorkerState::Done),
            ("0002".into(), WorkerState::Done)
        ]
    );
    // Clients hear about the gate (right after the worker's own status).
    until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::StageEnded { stage, status, summary, .. }
            if stage == "02_polish" && status == "review" && summary == "polished")
    })
    .await;
    // Let the ending's page writes land.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let stage = |name: &str| {
        std::fs::read_to_string(flow.join("note").join(name).join("_stage.md")).unwrap()
    };
    let first = stage("01_draft");
    assert!(
        first.contains("status: done")
            && first.contains("ticket: \"0001\"")
            && first.contains("summary: \"drafted\""),
        "{first}"
    );
    assert!(
        stage("02_polish").contains("status: review"),
        "{}",
        stage("02_polish")
    );
    assert!(flow.join("note/02_polish/final.md").is_file());
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
        3,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Error { id: Some(3), .. })
    })
    .await;
    let Some(ServerMessage::Error { message, .. }) = reply.last() else {
        panic!()
    };
    assert!(message.contains("02_polish waits for review"), "{message}");

    // At the gate, as an editor writes it (the board does the same through the API).
    let gate = |status: &str, note: &str| {
        let path = flow.join("note/02_polish/_stage.md");
        let raw = std::fs::read_to_string(&path).unwrap();
        let raw = raw
            .lines()
            .map(|l| match l {
                l if l.starts_with("status:") => format!("status: {status}"),
                l if l.starts_with("note:") && !note.is_empty() => format!("note: \"{note}\""),
                l => l.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, raw + "\n").unwrap();
    };
    gate("rerun", "Make it shorter.");
    let ended = |stage: &'static str, status: &'static str| move |e: &ServerMessage| matches!(e, ServerMessage::StageEnded { stage: s, status: t, .. } if s == stage && t == status);
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
    assert!(stage("02_polish").contains("summary: \"polished again\""));

    // Continue: the third stage starts and, without a gate, finishes the work.
    gate("approved", "");
    until(&mut socket, 60, ended("03_publish", "done")).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(stage("02_polish").contains("status: approved"));
    assert!(
        stage("03_publish").contains("status: done"),
        "{}",
        stage("03_publish")
    );
    assert!(flow.join("note/03_publish/out.md").is_file());
    // Approving again (or any edit of the page) starts nothing.
    gate("approved", "");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        !dir.path()
            .join("sites/orchestration/tickets/0005.md")
            .exists()
    );

    // Stale: a rule the draft read changes. Run reruns from there and goes on through the
    // stages after it, up to the gate.
    assert!(
        stage("01_draft").contains("inputs: {\"site://flow/_config/rules.md\":"),
        "{}",
        stage("01_draft")
    );
    std::fs::write(
        flow.join("_config/rules.md"),
        "---\ntitle: Rules\n---\n# Rules\n\nBe briefer.\n",
    )
    .unwrap();
    send(&mut socket, 4, ClientCommand::PipelineList).await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 4, .. })
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
        5,
        ClientCommand::WorkRun {
            work: "flow/note".into(),
            stage: None,
            session: None,
        },
    )
    .await;
    until(&mut socket, 60, ended("02_polish", "review")).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(stage("01_draft").contains("summary: \"drafted to the new rules\""));
    assert!(
        std::fs::read_to_string(flow.join("note/02_polish/final.md"))
            .unwrap()
            .contains("brief, polished")
    );
    send(&mut socket, 6, ClientCommand::PipelineList).await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 6, .. })
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
    // 03 read the old polish: stale until approving 02 runs it again.
    assert_eq!(stale, [false, false, true]);
}
