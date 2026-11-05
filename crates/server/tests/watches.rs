//! Files as the event bus: a change inside a site reaches subscribed clients as `page.changed`
//! (any file, editors' atomic saves included), and a session that asked to `watch` a path gets
//! a message when a matching change lands.

mod support;

use std::time::Duration;

use composer_protocol::{
    ChangedBy, ClientCommand, CommandResult, MessageSource, PageChangeKind, ServerMessage,
};
use composer_server::provider::ModelEvent;
use serde_json::json;
use support::*;

fn changed(events: &[ServerMessage]) -> Vec<(String, PageChangeKind)> {
    events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::PageChanged {
                url,
                kind,
                by: ChangedBy::External,
            } => Some((url.clone(), *kind)),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_file_changed_in_a_site_reaches_subscribed_clients() {
    let (dir, server, _p, addr) = start(vec![]).await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::Subscribe {
            urls: vec!["site://plans/**".into()],
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let file = dir.path().join("sites/plans/report.json");
    let url = "site://plans/report.json".to_string();
    let is_report = |e: &ServerMessage| matches!(e, ServerMessage::PageChanged { url: u, .. } if u == "site://plans/report.json");

    std::fs::write(&file, "{\"done\": 1}").unwrap();
    let events = until(&mut socket, is_report).await;
    assert_eq!(changed(&events), [(url.clone(), PageChangeKind::Created)]);

    // A burst of writes is one event.
    for n in 2..6 {
        std::fs::write(&file, format!("{{\"done\": {n}}}")).unwrap();
    }
    let events = until(&mut socket, is_report).await;
    assert_eq!(changed(&events), [(url.clone(), PageChangeKind::Modified)]);

    // An editor's save: a temp file renamed over it. The temp file makes no event.
    let temp = dir.path().join("sites/plans/report.json.tmp");
    std::fs::write(&temp, "{\"done\": 9}").unwrap();
    std::fs::rename(&temp, &file).unwrap();
    let events = until(&mut socket, is_report).await;
    assert_eq!(changed(&events), [(url.clone(), PageChangeKind::Modified)]);

    std::fs::remove_file(&file).unwrap();
    let events = until(&mut socket, is_report).await;
    assert_eq!(changed(&events), [(url.clone(), PageChangeKind::Deleted)]);

    // Nothing else came: no event for the temp file, no second one for the burst.
    tokio::time::sleep(Duration::from_millis(400)).await;
    send(&mut socket, 2, ClientCommand::SessionList).await;
    let rest = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    assert_eq!(changed(&rest), [], "{rest:?}");
}

#[tokio::test]
async fn a_watching_session_hears_when_files_land() {
    let (dir, server, provider, addr) = start(vec![
        vec![finished(vec![
            tool_call("w1", "watch", json!({ "pattern": "plans/renders/**" })),
            // Its own write doesn't wake (or end) its watch.
            tool_call(
                "w2",
                "write",
                json!({ "url": "site://plans/renders/notes.md", "content": "# Notes\n" }),
            ),
        ])],
        vec![ModelEvent::Content("Watching.".into()), finished(vec![])],
        vec![
            ModelEvent::Content("The renders are in.".into()),
            finished(vec![]),
        ],
    ])
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
        panic!("{reply:?}")
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
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: session.clone(),
            text: "tell me when the renders land".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(
        events.iter().any(|e| matches!(e,
            ServerMessage::ToolResult { result, is_error: false, .. } if result.contains("watching site://plans/renders/**"))),
        "{events:?}"
    );
    assert_eq!(server.watches.for_session(&session).len(), 1);

    // A render lands from outside composer (a job writing into the site).
    tokio::time::sleep(Duration::from_millis(700)).await;
    let renders = dir.path().join("sites/plans/renders");
    std::fs::write(renders.join("a.mp4"), b"mp4").unwrap();
    std::fs::write(renders.join("a.json"), "{}").unwrap();

    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let message = events
        .iter()
        .find_map(|e| match e {
            ServerMessage::MessageDone { message, .. } if message.source.is_some() => Some(message),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{events:?}"));
    assert_eq!(
        message.source,
        Some(MessageSource::Watch {
            pattern: "site://plans/renders/**".into()
        })
    );
    // Both files in one message; not its own notes.
    assert!(
        message
            .content
            .contains("- created site://plans/renders/a.mp4\n")
            && message
                .content
                .contains("- created site://plans/renders/a.json\n")
            && !message.content.contains("notes.md"),
        "{}",
        message.content
    );
    let last = provider.requests().last().unwrap().messages.clone();
    assert!(format!("{last:?}").contains("[files changed under your watch"));
    // Once: the watch ended with that message.
    assert!(server.watches.for_session(&session).is_empty());
}
