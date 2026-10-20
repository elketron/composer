//! Tools that create things outside existing sites: `new_site` and `new_project`.

mod support;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use composer_server::provider::{ModelEvent, ModelToolCall};
use serde_json::json;
use support::*;

/// Runs one turn in which the model makes `calls` (one step each); returns each call's
/// result text and whether it failed.
async fn run_tools(
    config: Config,
    calls: Vec<ModelToolCall>,
) -> (tempfile::TempDir, Vec<(String, bool)>) {
    let mut scripts: Vec<Vec<ModelEvent>> =
        calls.into_iter().map(|c| vec![finished(vec![c])]).collect();
    let count = scripts.len();
    scripts.push(vec![ModelEvent::Content("done".into()), finished(vec![])]);
    let (dir, server, _p, addr) = start_with(config, scripts).await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "default".into(),
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
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let results: Vec<(String, bool)> = events
        .into_iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result, is_error)),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), count, "{results:?}");
    (dir, results)
}

#[tokio::test]
async fn new_site_writes_a_homepage_and_lists_it_on_home() {
    let (dir, results) = run_tools(
        Config::defaults(),
        vec![
            tool_call(
                "a",
                "new_site",
                json!({ "name": "calendar", "title": "Calendar", "about": "reminders | due dates" }),
            ),
            tool_call("b", "new_site", json!({ "name": "calendar", "about": "again" })),
            tool_call("c", "new_site", json!({ "name": "../escape", "about": "no" })),
            tool_call(
                "d",
                "write",
                json!({ "url": "site://calendar/justfile", "content": "hello:\n    echo hi\n" }),
            ),
            tool_call("e", "new_app", json!({ "site": "calendar" })),
            tool_call("f", "list_sites", json!({})),
        ],
    )
    .await;
    let sites = dir.path().join("sites");
    let index = std::fs::read_to_string(sites.join("calendar/index.md")).unwrap();
    assert!(
        index.starts_with(
            "---\ntitle: \"Calendar\"\ntype: index\n---\n# Calendar\n\nreminders / due dates\n"
        ),
        "{index}"
    );
    for section in ["## Layout", "## Page format", "## Rules", "## Pages"] {
        assert!(index.contains(section), "{section}");
    }
    let home = std::fs::read_to_string(sites.join("home/index.md")).unwrap();
    assert!(
        home.contains("| [agents](site://agents) |")
            && home.contains("\n| [calendar](site://calendar) | reminders / due dates |\n"),
        "{home}"
    );

    assert!(
        !results[0].1 && results[0].0.contains("listed it on Home"),
        "{:?}",
        results[0]
    );
    assert!(
        results[1].1 && results[1].0.contains("already exists"),
        "{:?}",
        results[1]
    );
    assert!(results[2].1, "{:?}", results[2]);
    assert!(!sites.join("escape").exists() && !dir.path().join("escape").exists());
    // It takes actions and an app right away.
    assert!(
        sites.join("calendar/justfile").is_file(),
        "a justfile, not justfile.md"
    );
    assert!(!results[4].1, "{:?}", results[4]);
    assert!(
        results[5]
            .0
            .contains("site://calendar — Calendar [actions] [app, not built"),
        "{}",
        results[5].0
    );
}

#[tokio::test]
async fn new_project_makes_the_folder_and_registers_it() {
    let projects = tempfile::tempdir().unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    let template = |site: &str| json!({ "name": "lamp", "about": "a desk lamp", "template": site });
    let (dir, results) = run_tools(
        config,
        vec![
            tool_call("a", "new_project", json!({ "name": "notes", "about": "plain | notes" })),
            tool_call("b", "new_project", json!({ "name": "notes", "about": "again" })),
            tool_call("c", "new_project", json!({ "name": "../out", "about": "no" })),
            // The memory page exists but the folder does not: still refused.
            tool_call(
                "d0",
                "write",
                json!({ "url": "site://memory/projects/taken.md", "content": "# taken\n" }),
            ),
            tool_call("d", "new_project", json!({ "name": "taken", "about": "no" })),
            tool_call("e", "new_project", template("memory")),
            tool_call(
                "f",
                "write",
                json!({ "url": "site://plans/templates/project/BRIEF.md", "content": "# __PROJECT__ brief\n" }),
            ),
            tool_call("g", "new_project", template("plans")),
        ],
    )
    .await;
    let failed: Vec<bool> = results.iter().map(|r| r.1).collect();
    assert_eq!(
        failed,
        [false, true, true, false, true, true, false, false],
        "{results:?}"
    );
    assert!(results[1].0.contains("already exists"), "{}", results[1].0);
    assert!(
        results[5].0.contains("no templates/project/"),
        "{}",
        results[5].0
    );
    assert!(
        results[0].0.contains("registered it in the memory index")
            && results[0].0.contains("now this session's active project"),
        "{}",
        results[0].0
    );

    assert_eq!(
        std::fs::read_to_string(projects.path().join("notes/README.md")).unwrap(),
        "# notes\n\nplain / notes\n"
    );
    assert_eq!(
        std::fs::read_to_string(projects.path().join("lamp/BRIEF.md")).unwrap(),
        "# lamp brief\n"
    );
    assert!(results[4].0.contains("already exists"), "{}", results[4].0);
    assert!(!projects.path().join("taken").exists());
    assert!(!projects.path().parent().unwrap().join("out").exists());

    let memory = dir.path().join("sites/memory");
    let page = std::fs::read_to_string(memory.join("projects/lamp.md")).unwrap();
    assert!(
        page.contains("kind: project\n") && page.contains("workflow: plans\n"),
        "{page}"
    );
    assert!(
        page.contains(&format!("path: {}/lamp\n", projects.path().display())),
        "{page}"
    );
    let index = std::fs::read_to_string(memory.join("index.md")).unwrap();
    assert!(
        index.contains("## Projects registry\n\n| project | path | status | what it is |"),
        "{index}"
    );
    assert!(
        index.contains("| [notes](projects/notes.md) | `"),
        "{index}"
    );
    assert!(
        index.contains("| active | plain / notes |\n| [lamp](projects/lamp.md)"),
        "{index}"
    );
    assert!(
        index.contains("- [notes](projects/notes.md)\n- [lamp](projects/lamp.md)\n"),
        "{index}"
    );
    let registry = index.find("## Projects registry").unwrap();
    assert!(registry < index.find("## Pages").unwrap(), "{index}");
}
