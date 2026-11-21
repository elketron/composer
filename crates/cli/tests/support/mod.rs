//! Shared by the CLI tests: a fake OpenAI-compatible model that lists one model, and a
//! WebSocket client.
#![allow(dead_code)]

use std::time::Duration;

use axum::Json;
use axum::routing::get;
use composer_protocol::{ClientCommand, ClientRequest, ServerMessage};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

/// A fake provider's base URL: it lists one model and answers nothing else.
pub async fn fake_model() -> String {
    let app = axum::Router::new().route(
        "/v1/models",
        get(|| async { Json(json!({ "data": [{ "id": "fake" }] })) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}/v1")
}

pub type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub async fn send(socket: &mut Socket, id: u64, command: ClientCommand) {
    let json = serde_json::to_string(&ClientRequest { id, command }).unwrap();
    socket.send(Message::Text(json.into())).await.unwrap();
}

/// Events until `stop` matches (inclusive); panics after `secs`.
pub async fn until(
    socket: &mut Socket,
    secs: u64,
    stop: impl Fn(&ServerMessage) -> bool,
) -> Vec<ServerMessage> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let frame = tokio::time::timeout_at(deadline, socket.next())
            .await
            .expect("timed out waiting for an event")
            .unwrap()
            .unwrap();
        let Message::Text(text) = frame else { continue };
        let event: ServerMessage = serde_json::from_str(&text).unwrap();
        let done = stop(&event);
        events.push(event);
        if done {
            return events;
        }
    }
}
