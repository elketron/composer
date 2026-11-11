//! Learning from edits: at a gate, what you changed in the stage's output (against the kept
//! copy of what the agent wrote) is your edit; when enough works got edits at the same stage, a
//! model proposes amending the stage's contract, as an approval.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use composer_core::SiteUrl;
use composer_protocol::{ApprovalDecision, ApprovalKind, ChangedBy, ClientCommand, ServerMessage};
use composer_server::Config;
use composer_server::pipeline::learning::{edit_diff, gate_approved, keep_agent_output};
use composer_server::pipeline::{Work, pipeline};
use composer_server::provider::ModelEvent;
use composer_server::worker::ticket::set_field;
use serde_json::json;
use support::*;

const CONTRACT: &str = "---\ntitle: Draft\ntype: stage\ngate: true\n---\n# 01 · Draft\n\n## Process\n\nDraft the note.\n\n## Outputs\n\n- `01_draft/draft.md`\n";

#[tokio::test]
async fn recurring_edits_propose_an_amendment() {
    let amended = CONTRACT.replace(
        "Draft the note.",
        "Draft the note. Open with one sentence that says what it is for.",
    );
    let answer = json!({
        "pattern": "You cut the opening to one sentence that says what the note is for.",
        "target": "stages/01_draft.md",
        "text": amended,
    });
    let mut config = Config::defaults();
    config.pipelines.build_board = false;
    config.pipelines.learn_after = 2;
    let (dir, server, provider, addr) = start_with(
        config,
        vec![vec![
            ModelEvent::Content(format!("Here it is:\n{answer}")),
            finished(vec![]),
        ]],
    )
    .await;
    let mut socket = connect(addr, &server.token).await;
    let flow = dir.path().join("sites/flow");
    std::fs::create_dir_all(flow.join("stages")).unwrap();
    std::fs::write(
        flow.join("index.md"),
        "---\ntitle: Flow\ntype: pipeline\n---\n# Flow\n",
    )
    .unwrap();
    std::fs::write(flow.join("stages/01_draft.md"), CONTRACT).unwrap();

    // A work whose draft you left alone, then two whose draft you shortened before approving it.
    for (i, name) in ["c", "a", "b"].into_iter().enumerate() {
        send(
            &mut socket,
            i as u64 + 1,
            ClientCommand::WorkNew {
                pipeline: "flow".into(),
                name: name.into(),
                project: None,
                brief: None,
            },
        )
        .await;
        until(&mut socket, |e| {
            matches!(e, ServerMessage::Ok { .. } | ServerMessage::Error { .. })
        })
        .await;
        let work = Work::load(pipeline(&server, "flow").unwrap(), name).unwrap();
        let output = flow.join(name).join("01_draft/draft.md");
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::write(
            &output,
            "# Draft\n\nThis note, which was written today, is about many things.\n\nMore.\n",
        )
        .unwrap();
        // As the stage's end leaves it: the agent's copy kept, waiting for review.
        keep_agent_output(&work, "01_draft");
        assert!(edit_diff(&work, "01_draft").is_none(), "nothing edited yet");
        if name != "c" {
            std::fs::write(&output, "# Draft\n\nA note for the release.\n\nMore.\n").unwrap();
            let diff = edit_diff(&work, "01_draft").unwrap();
            assert!(
                diff.contains("-This note, which was written today")
                    && diff.contains("+A note for the release."),
                "{diff}"
            );
            assert!(
                diff.starts_with("--- a/draft.md\n+++ b/draft.md\n"),
                "{diff}"
            );
        }
        // Approve the gate on the work's page, then record what you changed (the board writes
        // the page; the next run asks the server to record the edit).
        let url = SiteUrl::new("flow", &format!("{name}/index.md")).unwrap();
        let raw = server.web.read_page(&url).unwrap().raw;
        let raw = set_field(&raw, "stage", "01_draft");
        let raw = set_field(&raw, "status", "approved");
        let raw = set_field(&raw, "done", "[\"01_draft\"]");
        server.write_page(&url, &raw, ChangedBy::App).unwrap();
        gate_approved(&server, &work, "01_draft");
    }

    // The second edited work asks the model, which proposes amending the contract.
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
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
    assert_eq!(*kind, ApprovalKind::Amendment);
    assert!(
        title.contains("amend stages/01_draft.md") && title.contains("one sentence"),
        "{title}"
    );
    assert!(
        body.contains("+Draft the note. Open with one sentence"),
        "{body}"
    );
    // The model saw both edits, not the unedited work.
    let prompt = format!("{:?}", provider.requests()[0]);
    assert!(
        prompt.contains("work=\\\"a\\\"")
            && prompt.contains("work=\\\"b\\\"")
            && !prompt.contains("work=\\\"c\\\""),
        "{prompt}"
    );
    let state = |name: &str| Work::load(pipeline(&server, "flow").unwrap(), name).unwrap();
    assert_eq!(
        state("a").stage_field("01_draft", "edited").as_deref(),
        Some("true")
    );
    assert_eq!(
        state("a").stage_field("01_draft", "learned").as_deref(),
        Some("proposed")
    );
    assert_eq!(state("c").stage_field("01_draft", "edited"), None);

    // The proposal is a page too; approving it amends the contract.
    let proposals: Vec<_> = std::fs::read_dir(flow.join("_proposals"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".md"))
        .collect();
    assert_eq!(proposals.len(), 1);
    send(
        &mut socket,
        9,
        ClientCommand::ApprovalRespond {
            request: request.clone(),
            decision: ApprovalDecision::Approve,
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalResolved { .. })
    })
    .await;
    for _ in 0..50 {
        if std::fs::read_to_string(flow.join("stages/01_draft.md")).unwrap() == amended {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        std::fs::read_to_string(flow.join("stages/01_draft.md")).unwrap(),
        amended
    );
    let proposal = std::fs::read_to_string(flow.join("_proposals").join(&proposals[0])).unwrap();
    assert!(
        proposal.contains("status: approved") && proposal.contains("type: proposal"),
        "{proposal}"
    );
}
