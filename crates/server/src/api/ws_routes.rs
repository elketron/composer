use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use composer_protocol::{ClientRequest, ServerMessage};
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use crate::api::Connection;
use crate::server::Server;

pub async fn upgrade(State(server): State<Arc<Server>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| run(server, socket))
}

/// Three tasks per socket: writer (outgoing queue → socket), forwarder
/// (broadcast → filter → queue) and the reader loop (commands → replies).
async fn run(server: Arc<Server>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Arc<ServerMessage>>();
    let connection = Arc::new(Connection::new(out_tx));

    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            let json = serde_json::to_string(&*message).expect("messages serialize");
            if sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    let mut events = server.hub.subscribe();
    let forward_to = connection.clone();
    let forwarder = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(message) => {
                    if forward_to.wants(&message) {
                        forward_to.send(message);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    forward_to.send(Arc::new(ServerMessage::Error {
                        id: None,
                        message: format!("missed {n} events; re-attach with `since` to catch up"),
                    }));
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    connection.send(Arc::new(server.slots.status()));
    for request in server.pending_approvals.all() {
        connection.send(Arc::new(request));
    }
    while let Some(Ok(message)) = stream.next().await {
        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => break,
            _ => continue,
        };
        let reply = match serde_json::from_str::<ClientRequest>(&text) {
            Ok(request) => connection.handle(&server, request),
            Err(e) => Some(ServerMessage::Error {
                id: None,
                message: format!("bad request: {e}"),
            }),
        };
        if let Some(reply) = reply {
            connection.send(Arc::new(reply));
        }
    }
    forwarder.abort();
    writer.abort();
}
