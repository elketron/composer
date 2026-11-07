//! The TUI as a desktop window's chat (ticket 0177): `window.hello` gets the window an id and
//! starts the real TUI (`composer --window <id>`) in a terminal of its own, whose output goes to
//! that window only; when the TUI shows a session, the window hears `window.changed` (its stage
//! follows); the window going ends its TUI.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use composer_protocol::{ClientCommand, ClientRequest, CommandResult, ServerMessage};
use composer_server::{Home, Host, Server, WorkspaceName, init_home};
use futures::{SinkExt, StreamExt};
use support::{Reply, Scripts, fake_model};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn a_window_gets_its_tui_and_follows_its_session() {
    let base_url = fake_model(Scripts {
        orchestrator: VecDeque::from(vec![Reply::Text("hi"); 5]),
        worker: VecDeque::new(),
        ..Scripts::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path().join("home"));
    init_home(&home).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // The TUI finds the server by the home's bind address.
    std::fs::write(
        home.global().join("config.toml"),
        format!("[server]\nbind = \"{addr}\"\n\n[provider]\nbase_url = \"{base_url}\"\n"),
    )
    .unwrap();
    // SAFETY: before the server starts; the only test in this binary.
    unsafe {
        std::env::set_var("COMPOSER_WORKER_EXE", env!("CARGO_BIN_EXE_composer"));
        std::env::set_var("COMPOSER_HOME", home.root());
    }
    let name = WorkspaceName::default_workspace();
    let server = Server::start(home.workspace(&name)).await.unwrap();
    let host = Arc::new(Host::new(home, BTreeMap::from([(name, server.clone())])).unwrap());
    let serving = host.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (mut window, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/w/default/ws?token={}",
        server.token
    ))
    .await
    .unwrap();
    let hello = ClientRequest {
        id: 1,
        command: ClientCommand::WindowHello {
            cols: 100,
            rows: 30,
        },
    };
    window
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    let (mut id, mut terminal, mut drew, mut followed) = (None, None, 0usize, None);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while followed.is_none() {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(left, window.next()).await
        else {
            panic!("no window.changed: window {id:?}, TUI output {drew} bytes")
        };
        match serde_json::from_str::<ServerMessage>(&text).unwrap() {
            ServerMessage::Ok {
                id: 1,
                result:
                    CommandResult::Window {
                        window: w,
                        terminal: t,
                    },
            } => {
                id = Some(w);
                terminal = Some(t);
            }
            ServerMessage::TerminalOutput {
                window: w, data, ..
            } => {
                assert_eq!(w, id, "the TUI's output goes to its window");
                drew += data.len();
            }
            ServerMessage::WindowChanged { window: w, session } => {
                assert_eq!(Some(w), id);
                followed = Some(session);
            }
            _ => {}
        }
    }
    assert!(drew > 0, "the TUI drew in its terminal");
    let session = followed.unwrap();
    assert!(server.sessions.get(&session).is_ok(), "a real session");

    // The window goes: its TUI ends.
    let terminal = terminal.unwrap();
    assert!(server.terminals.get(&terminal).is_some());
    drop(window);
    for _ in 0..50 {
        if server.terminals.get(&terminal).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        server.terminals.get(&terminal).is_none(),
        "the TUI outlived its window"
    );
}
