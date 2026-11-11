use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, State};
use axum::response::Response;
use composer_protocol::{ClientRequest, ServerMessage};
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use crate::api::{Connection, Credential};
use crate::apps::AppGrant;
use crate::server::Server;

/// The workspace's socket: `credential` is what the handshake was authenticated with.
pub async fn upgrade(
    State(server): State<Arc<Server>>,
    Extension(credential): Extension<Credential>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| run(server, socket, Client::Workspace(credential)))
}

/// Whose socket it is.
pub(crate) enum Client {
    /// A client of the workspace, with the credential it connected with.
    Workspace(Credential),
    /// An app, with what its token grants.
    App(AppGrant),
}

/// Three tasks per socket: writer (outgoing queue → socket), forwarder
/// (broadcast → filter → queue) and the reader loop (commands → replies).
/// An app's socket takes only what its token allows ([`Connection::for_app`]).
pub(crate) async fn run(server: Arc<Server>, socket: WebSocket, client: Client) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Arc<ServerMessage>>();
    let app = match &client {
        Client::App(grant) => Some(grant.clone()),
        Client::Workspace(_) => None,
    };
    let connection = Arc::new(match client {
        Client::App(grant) => Connection::for_app(out_tx, grant),
        Client::Workspace(credential) => Connection::new(out_tx, credential),
    });
    // An app's socket keeps its project open while it is there.
    if let Some(project) = app.as_ref().and_then(|g| g.project.as_ref()) {
        connection.open(&server, &project.name, &project.folder);
    }

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
    connection.close_all(&server);
    forwarder.abort();
    writer.abort();
}
