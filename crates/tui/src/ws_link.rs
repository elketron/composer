//! The WebSocket connection: reconnects forever, turns frames into actions.

use std::time::Duration;

use composer_protocol::{ClientRequest, ServerMessage};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::action::Action;
use crate::dispatcher::Dispatcher;

/// Starts the connection task. Requests sent while disconnected are dropped
/// (the stores refuse to send then anyway).
pub fn spawn(
    server_url: &str,
    token: &str,
    dispatcher: Dispatcher,
) -> mpsc::UnboundedSender<ClientRequest> {
    let ws_url = format!(
        "{}/ws?token={token}",
        server_url.trim_end_matches('/').replacen("http", "ws", 1)
    );
    let (tx, mut rx) = mpsc::unbounded_channel::<ClientRequest>();
    tokio::spawn(async move {
        loop {
            match tokio_tungstenite::connect_async(&ws_url).await {
                Ok((socket, _)) => {
                    let (mut sink, mut stream) = socket.split();
                    while rx.try_recv().is_ok() {} // stale requests from before
                    dispatcher.dispatch(Action::Connected);
                    let reason = loop {
                        tokio::select! {
                            request = rx.recv() => {
                                let Some(request) = request else { return };
                                let json = serde_json::to_string(&request).expect("requests serialize");
                                if let Err(e) = sink.send(Message::Text(json.into())).await {
                                    break e.to_string();
                                }
                            }
                            frame = stream.next() => match frame {
                                Some(Ok(Message::Text(text))) => {
                                    if let Ok(message) = serde_json::from_str::<ServerMessage>(&text) {
                                        dispatcher.dispatch(Action::Server(message));
                                    }
                                }
                                Some(Ok(_)) => {}
                                Some(Err(e)) => break e.to_string(),
                                None => break "connection closed".to_string(),
                            }
                        }
                    };
                    dispatcher.dispatch(Action::Disconnected(reason));
                }
                Err(e) => dispatcher.dispatch(Action::Disconnected(e.to_string())),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
    tx
}
