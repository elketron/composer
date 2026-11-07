//! The client crate against a real server (ticket 0180): a connection opened before the server is
//! up keeps trying and connects once it is; `request` gets its reply (and an error reply as
//! `Err`); events come as events. And the crate itself never links the server (only its tests do).

use std::sync::Arc;
use std::time::Duration;

use composer_client::{Connection, Endpoint, Event};
use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::provider::{ModelRequest, ModelStream, Provider};
use composer_server::{Config, Server, WorkspaceHome, init_workspace};

/// A model that never answers: the connection is what's tested.
struct Silent;

#[async_trait::async_trait]
impl Provider for Silent {
    fn model(&self) -> &str {
        "silent"
    }

    async fn stream(&self, _request: ModelRequest) -> Result<ModelStream, String> {
        Err("no model here".into())
    }
}

#[tokio::test]
async fn a_connection_reconnects_and_pairs_replies() {
    let dir = tempfile::tempdir().unwrap();
    let home = WorkspaceHome::new(dir.path());
    init_workspace(&home).await.unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let server = Server::new(home, Config::defaults(), Arc::new(Silent)).unwrap();

    // Open first: nothing listens yet.
    let (connection, mut events) = Connection::open(Endpoint::new(
        format!("http://{addr}"),
        server.token.clone(),
    ));
    let first = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, Event::Disconnected(_)), "{first:?}");
    assert!(
        connection
            .request(ClientCommand::SessionList)
            .await
            .is_err()
    );

    // The server comes up: it connects on its own.
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let serving = server.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    loop {
        let event = tokio::time::timeout(Duration::from_secs(15), events.recv())
            .await
            .expect("connected within the backoff")
            .unwrap();
        if event == Event::Connected {
            break;
        }
    }

    // A request and its reply; an error reply.
    let created = connection
        .request(ClientCommand::SessionCreate {
            agent: "default".into(),
            project: None,
        })
        .await
        .unwrap();
    let CommandResult::Session { session } = created else {
        panic!("{created:?}")
    };
    let listed = connection
        .request(ClientCommand::SessionList)
        .await
        .unwrap();
    assert!(
        matches!(listed, CommandResult::Sessions { sessions } if sessions.iter().any(|s| s.id == session.id))
    );
    let missing = connection
        .request(ClientCommand::DeskGet {
            session: "nope".into(),
        })
        .await;
    assert!(missing.is_err());

    // Events: attached to the session, a desk change arrives as an event.
    connection
        .request(ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        })
        .await
        .unwrap();
    connection
        .request(ClientCommand::DeskOpen {
            session: session.id.clone(),
            tab: composer_protocol::DeskTab::Page {
                url: "site://memory".into(),
            },
            focus: true,
        })
        .await
        .unwrap();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        if let Event::Message(ServerMessage::DeskChanged { session: s, .. }) = event {
            assert_eq!(s, session.id);
            break;
        }
    }
}

#[test]
fn the_crate_never_links_the_server() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .unwrap()
        .parse::<toml::Table>()
        .unwrap();
    let dependencies = manifest["dependencies"].as_table().unwrap();
    assert!(
        !dependencies.contains_key("composer-server"),
        "composer-client depends on the server: the desktop would link it"
    );
}

#[tokio::test]
async fn a_request_in_flight_fails_when_the_connection_drops() {
    use futures::StreamExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // A server that takes one request and hangs up without answering.
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let _ = socket.next().await;
    });
    let (connection, mut events) = Connection::open(Endpoint::new(format!("http://{addr}"), "t"));
    assert_eq!(events.recv().await.unwrap(), Event::Connected);
    let answer = tokio::time::timeout(
        Duration::from_secs(5),
        connection.request(ClientCommand::SessionList),
    )
    .await
    .expect("the request ends with the connection, not hangs");
    assert!(answer.unwrap_err().starts_with("disconnected"));
}
