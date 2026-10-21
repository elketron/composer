//! Pipelines: made from templates, works made in them (folders with a brief and a folder per
//! stage), their stages' state in the project overview, `_config` overrides; and the agent tools.

mod support;

use composer_core::SiteUrl;
use composer_protocol::{ChangedBy, ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use composer_server::pipeline::{Work, pipeline};
use composer_server::provider::ModelEvent;
use serde_json::json;
use support::*;

async fn command(
    socket: &mut Socket,
    id: u64,
    command: ClientCommand,
) -> Result<CommandResult, String> {
    send(socket, id, command).await;
    let reply = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    match reply.last() {
        Some(ServerMessage::Ok { result, .. }) => Ok(result.clone()),
        Some(ServerMessage::Error { message, .. }) => Err(message.clone()),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn pipelines_and_works() {
    let projects = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(projects.path().join("game")).unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    // Not here: bun and the network.
    config.pipelines.build_board = false;
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let mut socket = connect(addr, &server.token).await;

    // The templates `init` installed; no pipelines yet.
    let Ok(CommandResult::Pipelines {
        pipelines,
        templates,
        ..
    }) = command(&mut socket, 1, ClientCommand::PipelineList).await
    else {
        panic!()
    };
    assert!(pipelines.is_empty());
    let kinds: Vec<&str> = templates.iter().map(|t| t.site.as_str()).collect();
    assert_eq!(kinds, ["coding", "research", "video"]);
    let stages: Vec<&str> = templates[0]
        .stages
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(stages, ["01_spec", "02_plan", "03_implement", "04_review"]);

    // A pipeline from a template, listed on Home; a second one of a kind under another name.
    let new_pipeline = |kind: &str, name: Option<&str>| ClientCommand::PipelineNew {
        kind: kind.into(),
        name: name.map(Into::into),
    };
    let Ok(CommandResult::Pipeline { pipeline: coding }) =
        command(&mut socket, 2, new_pipeline("coding", None)).await
    else {
        panic!()
    };
    assert_eq!(
        (coding.site.as_str(), coding.needs_project, coding.works),
        ("coding", true, 0)
    );
    let implement = &coding.stages[2];
    assert_eq!(
        (
            implement.title.as_str(),
            implement.gate,
            implement.worktree,
            implement.harness.as_deref()
        ),
        ("Implement", false, true, Some("claude-code"))
    );
    assert!(coding.stages[0].gate);
    let home = std::fs::read_to_string(dir.path().join("sites/home/index.md")).unwrap();
    assert!(home.contains("| [coding](site://coding) |"), "{home}");
    let index = std::fs::read_to_string(dir.path().join("sites/coding/index.md")).unwrap();
    assert!(index.contains("site://coding/<work>/") && !index.contains("__PIPELINE__"));
    // Indexed as it was written.
    assert!(
        server
            .index
            .read()
            .unwrap()
            .get(&SiteUrl::new("coding", "stages/02_plan.md").unwrap())
            .is_some()
    );

    // Its board app, named for the site, and the action that builds it (approved).
    let board = std::fs::read_to_string(dir.path().join("sites/coding/ui/package.json")).unwrap();
    assert!(board.contains("\"name\": \"coding-board\""), "{board}");
    assert!(dir.path().join("sites/coding/ui/src/App.svelte").is_file());
    let justfile = std::fs::read_to_string(dir.path().join("sites/coding/justfile")).unwrap();
    assert!(justfile.contains("ui-build:"), "{justfile}");
    let hash = composer_server::actions::Justfile::load(&dir.path().join("sites/coding"))
        .await
        .unwrap()
        .hash("ui-build");
    let approved = || server.approvals.is_approved("coding", "ui-build", &hash);
    for _ in 0..50 {
        if approved() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(approved());
    let err = command(&mut socket, 3, new_pipeline("coding", None))
        .await
        .unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    let err = command(&mut socket, 4, new_pipeline("podcast", None))
        .await
        .unwrap_err();
    assert!(err.contains("coding, research, video"), "{err}");
    let Ok(CommandResult::Pipeline { pipeline: shorts }) =
        command(&mut socket, 5, new_pipeline("video", Some("shorts"))).await
    else {
        panic!()
    };
    assert_eq!(
        (shorts.site.as_str(), shorts.needs_project),
        ("shorts", false)
    );

    // Works.
    let new_work = |pipeline: &str, name: &str, project: Option<&str>| ClientCommand::WorkNew {
        pipeline: pipeline.into(),
        name: name.into(),
        project: project.map(Into::into),
        brief: Some("Save slots, three of them.".into()),
    };
    let err = command(&mut socket, 6, new_work("coding", "save-system", None))
        .await
        .unwrap_err();
    assert!(err.contains("links a project"), "{err}");
    let err = command(&mut socket, 7, new_work("coding", "stages", Some("game")))
        .await
        .unwrap_err();
    assert!(err.contains("the pipeline's own"), "{err}");
    let err = command(&mut socket, 8, new_work("home", "x", None))
        .await
        .unwrap_err();
    assert!(err.contains("not a pipeline"), "{err}");
    let Ok(CommandResult::Work { work }) = command(
        &mut socket,
        9,
        new_work("coding", "save-system", Some("game")),
    )
    .await
    else {
        panic!()
    };
    assert_eq!(work.url, "site://coding/save-system/index.md");
    assert_eq!(work.status.as_deref(), Some("01 spec · waiting"));
    assert_eq!(work.stages.len(), 4);
    let game = projects.path().join("game").canonicalize().unwrap();
    assert_eq!(work.project.as_deref(), Some(game.to_str().unwrap()));
    let err = command(
        &mut socket,
        10,
        new_work("coding", "save-system", Some("game")),
    )
    .await
    .unwrap_err();
    assert!(err.contains("already exists"), "{err}");
    assert!(
        command(&mut socket, 11, new_work("shorts", "trailer", None))
            .await
            .is_ok()
    );

    let work_dir = dir.path().join("sites/coding/save-system");
    let index = std::fs::read_to_string(work_dir.join("index.md")).unwrap();
    assert!(
        index.contains("type: work\npipeline: coding\n")
            && index.contains("Save slots, three of them."),
        "{index}"
    );
    let stage = std::fs::read_to_string(work_dir.join("03_implement/_stage.md")).unwrap();
    assert!(
        stage.contains("status: waiting\nharness: claude-code\n"),
        "{stage}"
    );
    let listed = std::fs::read_to_string(dir.path().join("sites/coding/index.md")).unwrap();
    assert!(
        listed.contains("- [save-system](save-system/index.md) · `"),
        "{listed}"
    );

    // The project's overview shows the work and where it is; a finished spec moves it on.
    let overview = |_| ClientCommand::ProjectOverview {
        project: "game".into(),
    };
    let Ok(CommandResult::Project { overview: o }) = command(&mut socket, 12, overview(())).await
    else {
        panic!()
    };
    assert_eq!(o.works.len(), 1);
    assert_eq!(o.works[0].status.as_deref(), Some("01 spec · waiting"));
    let spec = SiteUrl::new("coding", "save-system/01_spec/_stage.md").unwrap();
    let text = std::fs::read_to_string(work_dir.join("01_spec/_stage.md"))
        .unwrap()
        .replace("status: waiting", "status: done");
    server.write_page(&spec, &text, ChangedBy::App).unwrap();
    let Ok(CommandResult::Project { overview: o }) = command(&mut socket, 13, overview(())).await
    else {
        panic!()
    };
    assert_eq!(o.works[0].status.as_deref(), Some("02 plan · waiting"));
    assert_eq!(o.works[0].stages[0].status, "done");

    let Ok(CommandResult::Pipelines {
        pipelines,
        works: all,
        ..
    }) = command(&mut socket, 14, ClientCommand::PipelineList).await
    else {
        panic!()
    };
    let works: Vec<(&str, u32)> = pipelines
        .iter()
        .map(|p| (p.site.as_str(), p.works))
        .collect();
    assert_eq!(works, [("coding", 1), ("shorts", 1)]);

    // Every work, with its stages and what they wrote.
    let urls: Vec<&str> = all.iter().map(|w| w.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "site://coding/save-system/index.md",
            "site://shorts/trailer/index.md"
        ]
    );
    std::fs::write(work_dir.join("01_spec/spec.md"), "# Spec\n").unwrap();
    let Ok(CommandResult::Project { overview: o }) = command(&mut socket, 15, overview(())).await
    else {
        panic!()
    };
    assert_eq!(
        o.works[0].stages[0].outputs,
        ["site://coding/save-system/01_spec/spec.md"]
    );

    // Inputs: the brief, `_config` (the work's own file wins), the work's files.
    let work = Work::load(pipeline(&server, "coding").unwrap(), "save-system").unwrap();
    let resolve = |path: &str| work.resolve(path).unwrap().to_string();
    assert_eq!(resolve("brief"), "site://coding/save-system/index.md");
    assert_eq!(
        resolve("_config/conventions.md"),
        "site://coding/_config/conventions.md"
    );
    std::fs::create_dir_all(work_dir.join("_config")).unwrap();
    std::fs::write(work_dir.join("_config/conventions.md"), "# here\n").unwrap();
    assert_eq!(
        resolve("_config/conventions.md"),
        "site://coding/save-system/_config/conventions.md"
    );
    assert_eq!(
        resolve("01_spec/spec.md"),
        "site://coding/save-system/01_spec/spec.md"
    );
    let contract = work.pipeline.stage("02_plan").unwrap();
    assert_eq!(contract.inputs[0].path, "01_spec/spec.md");
    assert_eq!(contract.outputs, ["02_plan/plan.md"]);
}

#[tokio::test]
async fn the_agent_tools() {
    let projects = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(projects.path().join("site")).unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    let calls = vec![
        tool_call("a", "new_pipeline", json!({ "kind": "research" })),
        tool_call(
            "b",
            "new_work",
            json!({ "pipeline": "research", "name": "ssr", "title": "Is SSR worth it?", "brief": "For the website." }),
        ),
        tool_call(
            "c",
            "new_work",
            json!({ "pipeline": "site://research/", "name": "../x" }),
        ),
    ];
    let mut scripts: Vec<Vec<ModelEvent>> =
        calls.into_iter().map(|c| vec![finished(vec![c])]).collect();
    scripts.push(vec![ModelEvent::Content("done".into()), finished(vec![])]);
    let (dir, server, _provider, addr) = start_with(config, scripts).await;
    let mut socket = connect(addr, &server.token).await;
    let Ok(CommandResult::Session { session }) = command(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "default".into(),
            project: None,
        },
    )
    .await
    else {
        panic!()
    };
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
            text: "make it".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let results: Vec<(String, bool)> =
        until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. }))
            .await
            .into_iter()
            .filter_map(|e| match e {
                ServerMessage::ToolResult {
                    result, is_error, ..
                } => Some((result, is_error)),
                _ => None,
            })
            .collect();
    assert_eq!(
        results.iter().map(|r| r.1).collect::<Vec<_>>(),
        [false, false, true],
        "{results:?}"
    );
    assert!(
        results[0].0.contains("01_question (gate)"),
        "{}",
        results[0].0
    );
    assert!(
        results[1].0.contains("site://research/ssr/index.md"),
        "{}",
        results[1].0
    );
    let index = std::fs::read_to_string(dir.path().join("sites/research/ssr/index.md")).unwrap();
    assert!(
        index.contains("title: \"Is SSR worth it?\"") && !index.contains("project:"),
        "{index}"
    );
}
