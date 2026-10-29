//! Pages must reach the search index as soon as they land, however they were written:
//! through the server (`write` tool / app putFile) or straight to the filesystem by
//! another process — including into a folder created moments before (ticket 0071: the
//! watcher's window on fresh folders made a written page unsearchable until an edit).

mod support;

use composer_core::{Query, SiteUrl};
use composer_protocol::{ChangedBy, ClientCommand, CommandResult, ServerMessage};
use composer_server::provider::ModelEvent;
use composer_server::{Config, Server};
use serde_json::json;
use support::*;

fn search(server: &Server, query: &str) -> Vec<String> {
    server
        .index
        .read()
        .expect("index lock")
        .search(&Query::parse(query))
        .into_iter()
        .map(|h| h.url.to_string())
        .collect()
}

#[tokio::test]
async fn a_page_created_by_write_is_searchable_immediately() {
    let (_dir, server, _p, _addr) = start(vec![]).await;

    let url = SiteUrl::new("memory", "topics/scratch-test.md").unwrap();
    let text = "---\ntitle: Test\ntype: scratch-test\n---\n# Test\n";
    let (written, created) = server.write_page(&url, text, ChangedBy::Agent).unwrap();
    assert!(created, "{url} should be new");
    assert!(
        search(&server, "type:scratch-test").contains(&written.to_string()),
        "a page created by write must be searchable without a further edit"
    );

    // The reverse the ticket asks about: the app putFile path (`Server::write_file`) —
    // markdown through it is the same code path, so it must index too.
    let url = SiteUrl::new("plans", "scratch-test2/x.md").unwrap();
    let text = "---\ntitle: Test\ntype: scratch-test2\n---\n# Test\n";
    let (written, created) = server
        .write_file(&url, text.as_bytes(), ChangedBy::App)
        .unwrap();
    assert!(created, "{url} should be new");
    assert!(
        search(&server, "type:scratch-test2").contains(&written.to_string()),
        "a page created through putFile must be searchable"
    );

    // A non-markdown file next to it (a versions.json) is not a page and stays out of the
    // index; writing one must not lose the page that is in it.
    server
        .write_file(
            &SiteUrl::new("plans", "scratch-test2/versions.json").unwrap(),
            b"[]",
            ChangedBy::App,
        )
        .unwrap();
    assert!(
        search(&server, "type:scratch-test2").contains(&written.to_string()),
        "writing a non-page next to it must not lose the page"
    );
}

#[tokio::test]
async fn a_page_written_by_another_process_is_searchable() {
    let (dir, server, _p, _addr) = start(vec![]).await;
    // The watcher comes up when serve runs; let it before writing anything.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // An existing folder: the watch covers it, the file event arrives.
    std::fs::write(
        dir.path().join("sites/memory/w1.md"),
        "---\ntitle: W1\ntype: scratch-w1\n---\n# W1\n",
    )
    .unwrap();

    // A folder created right now: files written into the gap before the watcher attaches
    // to the new folder must still reach the index.
    let fresh = dir.path().join("sites/memory/freshdir");
    std::fs::create_dir(&fresh).unwrap();
    std::fs::write(
        fresh.join("w2.md"),
        "---\ntitle: W2\ntype: scratch-w2\n---\n# W2\n",
    )
    .unwrap();

    // A site created whole, straight on the filesystem.
    let site = dir.path().join("sites/newsites");
    std::fs::create_dir(&site).unwrap();
    std::fs::write(
        site.join("w3.md"),
        "---\ntitle: W3\ntype: scratch-w3\n---\n# W3\n",
    )
    .unwrap();

    // Give the watcher a moment, then look. One assert per leg so each seam fails on its own.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    assert!(
        !search(&server, "type:scratch-w1").is_empty(),
        "a file written into an existing folder is searchable"
    );
    assert!(
        !search(&server, "type:scratch-w2").is_empty(),
        "a file written into a folder created moments before is searchable"
    );
    assert!(
        !search(&server, "type:scratch-w3").is_empty(),
        "a file in a site created whole from another process is searchable"
    );
}

/// The ticket's repro at the tool level: write a page, then search for it.
#[tokio::test]
async fn the_write_tool_then_the_search_tool_find_the_page() {
    let (_dir, server, _p, addr) = start_with(
        Config::defaults(),
vec![
    vec![finished(vec![tool_call("w", "write", json!({"url": "site://memory/tools/scratch-e2e", "content": "---\ntitle: E2E\ntype: scratch-e2e\n---\n# E2E\n"}))])],
    vec![finished(vec![tool_call("s", "search", json!({"query": "type:scratch-e2e"}))])],
    vec![ModelEvent::Content("done".into()), finished(vec![])],
],
    )
    .await;
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
            text: "write it then find it".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let results: Vec<String> = events
        .into_iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult { result, .. } => Some(result),
            _ => None,
        })
        .collect();
    println!("RESULTS: {results:?}");
    println!(
        "FILE: {:?}",
        std::fs::read_to_string(_dir.path().join("sites/memory/tools/scratch-e2e.md"))
    );
    println!("DIRECT: {:?}", search(&server, "type:scratch-e2e"));
    assert_eq!(results.len(), 2, "{results:?}");
    assert!(
        results[0].starts_with("created "),
        "write should create: {}",
        results[0]
    );
    assert!(
        results[1].contains("site://memory/tools/scratch-e2e.md"),
        "search must find the page write just created, without any edit in between: {}",
        results[1]
    );
}
