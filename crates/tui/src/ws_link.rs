//! The WebSocket connection: the client crate's [`Connection`] (it reconnects), its events turned
//! into actions.

use composer_client::{Connection, Endpoint, Event};
use composer_protocol::ClientRequest;
use tokio::sync::mpsc;

use crate::action::Action;
use crate::dispatcher::Dispatcher;

/// Starts the connection. Requests sent while disconnected are dropped (the stores refuse to send
/// then anyway).
pub fn spawn(
    server_url: &str,
    token: &str,
    dispatcher: Dispatcher,
) -> mpsc::UnboundedSender<ClientRequest> {
    let (connection, mut events) = Connection::open(Endpoint::new(server_url, token));
    let (tx, mut rx) = mpsc::unbounded_channel::<ClientRequest>();
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            let _ = connection.send(request);
        }
    });
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            dispatcher.dispatch(match event {
                Event::Connected => Action::Connected,
                Event::Disconnected(reason) => Action::Disconnected(reason),
                Event::Message(message) => Action::Server(message),
            });
        }
    });
    tx
}
