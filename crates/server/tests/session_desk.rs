//! Sessions own the desk (ticket 0173): `desk.open`/`focus`/`move`/`close` change a session's
//! tabs (an equal tab is focused, not doubled), every change reaches the clients attached to the
//! session as `desk.changed`, and the desk is still there after a restart.

mod support;

use std::sync::Arc;
use std::time::Duration;

use composer_protocol::{ClientCommand, CommandResult, Desk, DeskBy, DeskTab, ServerMessage};
use composer_server::{Config, Server, WorkspaceHome};
use support::*;

async fn desk(socket: &mut Socket, id: u64, command: ClientCommand) -> Result<Desk, String> {
    send(socket, id, command).await;
    let events = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    match events.last() {
        Some(ServerMessage::Ok {
            result: CommandResult::Desk { desk },
            ..
        }) => Ok(desk.clone()),
        Some(ServerMessage::Error { message, .. }) => Err(message.clone()),
        other => panic!("{other:?}"),
    }
}

fn ids(desk: &Desk) -> Vec<&str> {
    desk.tabs.iter().map(|t| t.id.as_str()).collect()
}

#[tokio::test]
async fn a_sessions_desk_comes_back_with_it() {
    let (dir, server, _provider, addr) = start(vec![]).await;
    let session = server.sessions.create("default", None).unwrap();
    let id = session.id().to_string();
    let mut attached = connect(addr, &server.token).await;
    send(
        &mut attached,
        1,
        ClientCommand::SessionAttach {
            session: id.clone(),
            since: None,
        },
    )
    .await;
    until(&mut attached, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let mut other = connect(addr, &server.token).await;

    // A new session: an empty desk (the project home).
    assert_eq!(
        desk(
            &mut other,
            2,
            ClientCommand::DeskGet {
                session: id.clone()
            }
        )
        .await,
        Ok(Desk::default())
    );
    let open = |tab: DeskTab| ClientCommand::DeskOpen {
        session: id.clone(),
        tab,
        focus: true,
    };
    let cad = DeskTab::App {
        site: "cad".into(),
        project: Some("game".into()),
    };
    desk(&mut other, 3, open(cad.clone())).await.unwrap();
    desk(
        &mut other,
        4,
        open(DeskTab::Page {
            url: "site://plans/x".into(),
        }),
    )
    .await
    .unwrap();
    // Again: focused, not doubled.
    let now = desk(&mut other, 5, open(cad)).await.unwrap();
    assert_eq!(ids(&now), ["app:cad@game", "page:site://plans/x"]);
    assert_eq!(now.active.as_deref(), Some("app:cad@game"));
    let moved = desk(
        &mut other,
        6,
        ClientCommand::DeskMove {
            session: id.clone(),
            tab: "page:site://plans/x".into(),
            to: 0,
        },
    )
    .await
    .unwrap();
    assert_eq!(ids(&moved), ["page:site://plans/x", "app:cad@game"]);

    // The attached client heard every change, by the user.
    let mut heard = 0;
    while heard < 4 {
        let event = tokio::time::timeout(Duration::from_secs(5), next_event(&mut attached))
            .await
            .unwrap();
        if let ServerMessage::DeskChanged { session, by, .. } = event {
            assert_eq!((session.as_str(), by), (id.as_str(), DeskBy::User));
            heard += 1;
        }
    }
    // The other client, not attached, heard none (only replies).
    let quiet = tokio::time::timeout(Duration::from_millis(300), next_event(&mut other)).await;
    assert!(
        !matches!(quiet, Ok(ServerMessage::DeskChanged { .. })),
        "{quiet:?}"
    );

    // Bad tabs and unknown sessions say so.
    let bad = desk(
        &mut other,
        7,
        ClientCommand::DeskClose {
            session: id.clone(),
            tab: "app:nope".into(),
        },
    )
    .await;
    assert!(bad.unwrap_err().contains("no tab"));
    let unknown = desk(
        &mut other,
        8,
        ClientCommand::DeskGet {
            session: "nope".into(),
        },
    )
    .await;
    assert!(unknown.is_err());

    // After a restart: the same desk.
    let again = Server::new(
        WorkspaceHome::new(dir.path()),
        Config::defaults(),
        Arc::new(MockProvider::new(vec![])),
    )
    .unwrap();
    assert_eq!(again.sessions.get(&id).unwrap().desk(), moved);
}

#[tokio::test]
async fn the_live_view_opens_by_hand_only_with_a_viewer() {
    let open = |id: &str| ClientCommand::DeskOpen {
        session: id.to_string(),
        tab: DeskTab::LiveView,
        focus: true,
    };
    // No viewer: refused, and says how to get one.
    let (_dir, server, _provider, addr) = start(vec![]).await;
    let id = server
        .sessions
        .create("default", None)
        .unwrap()
        .id()
        .to_string();
    let mut client = connect(addr, &server.token).await;
    let refused = desk(&mut client, 1, open(&id)).await.unwrap_err();
    assert!(refused.contains("[viewer]"), "{refused}");
    assert_eq!(server.sessions.get(&id).unwrap().desk(), Desk::default());

    // With one: a tab `live`, shown; again, not doubled.
    let mut config = Config::defaults();
    config.viewer.url = "http://127.0.0.1:9/view".into();
    let (_dir, server, _provider, addr) = start_with(config, vec![]).await;
    let id = server
        .sessions
        .create("default", None)
        .unwrap()
        .id()
        .to_string();
    let mut client = connect(addr, &server.token).await;
    desk(&mut client, 1, open(&id)).await.unwrap();
    let now = desk(&mut client, 2, open(&id)).await.unwrap();
    assert_eq!(ids(&now), ["live"]);
    assert_eq!(now.active.as_deref(), Some("live"));
    assert_eq!(now.tabs[0].tab, DeskTab::LiveView);
}
