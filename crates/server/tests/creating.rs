//! Tools that create things outside existing sites: `new_site`.

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
    site: &str,
    calls: Vec<ModelToolCall>,
) -> (tempfile::TempDir, Vec<(String, bool)>) {
    // Enter the site first: its verbs exist only inside.
    let mut scripts: Vec<Vec<ModelEvent>> = vec![
        vec![finished(vec![tool_call(
            "enter",
            "set_site",
            json!({ "site": site }),
        )])],
        vec![ModelEvent::Content("in site".into()), finished(vec![])],
    ];
    let count = calls.len();
    scripts.extend(calls.into_iter().map(|c| vec![finished(vec![c])]));
    scripts.push(vec![ModelEvent::Content("done".into()), finished(vec![])]);
    let (dir, server, _p, addr) = start_with(config, scripts).await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            kind: composer_protocol::RunKind::Agentic,
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
    let message = |text: &str| ClientCommand::MessageSend {
        session: session.id.clone(),
        text: text.into(),
        pages: vec![],
        images: vec![],
        attachments: vec![],
    };
    send(&mut socket, 3, message("enter the site")).await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    send(&mut socket, 4, message("make it")).await;
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
        "home",
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
            tool_call("e", "list_sites", json!({})),
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
        home.contains("| [memory](site://memory) |")
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
    // It takes actions right away.
    assert!(
        sites.join("calendar/justfile").is_file(),
        "a justfile, not justfile.md"
    );
    assert!(
        results[4]
            .0
            .contains("site://calendar — Calendar [actions]"),
        "{}",
        results[4].0
    );
}
