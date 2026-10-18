#![allow(dead_code)]

mod mock_provider;

use std::net::SocketAddr;
use std::sync::Arc;

use composer_protocol::{ClientCommand, ClientRequest, ServerMessage};
use composer_server::provider::{ModelEvent, ModelToolCall};
use composer_server::{Config, Home, Server, init_home};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub use mock_provider::MockProvider;

pub type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// An initialized home and a running server on a random port.
pub async fn start(
    scripts: Vec<Vec<ModelEvent>>,
) -> (
    tempfile::TempDir,
    Arc<Server>,
    Arc<MockProvider>,
    SocketAddr,
) {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let provider = Arc::new(MockProvider::new(scripts));
    let server = Server::new(home, Config::defaults(), provider.clone()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = server.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    (dir, server, provider, addr)
}

pub async fn connect(addr: SocketAddr, token: &str) -> Socket {
    tokio_tungstenite::connect_async(format!("ws://{addr}/ws?token={token}"))
        .await
        .unwrap()
        .0
}

pub async fn send(socket: &mut Socket, id: u64, command: ClientCommand) {
    let json = serde_json::to_string(&ClientRequest { id, command }).unwrap();
    socket.send(Message::Text(json.into())).await.unwrap();
}

pub async fn next_event(socket: &mut Socket) -> ServerMessage {
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .expect("timed out waiting for an event")
            .unwrap()
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

/// Collects events until `stop` matches (inclusive).
pub async fn until(
    socket: &mut Socket,
    stop: impl Fn(&ServerMessage) -> bool,
) -> Vec<ServerMessage> {
    let mut events = Vec::new();
    loop {
        let event = next_event(socket).await;
        let done = stop(&event);
        events.push(event);
        if done {
            return events;
        }
    }
}

pub fn tool_call(id: &str, name: &str, args: serde_json::Value) -> ModelToolCall {
    ModelToolCall {
        id: id.into(),
        name: name.into(),
        arguments: args.to_string(),
    }
}

pub fn finished(tool_calls: Vec<ModelToolCall>) -> ModelEvent {
    ModelEvent::Finished {
        tool_calls,
        usage: None,
        finish_reason: None,
    }
}
