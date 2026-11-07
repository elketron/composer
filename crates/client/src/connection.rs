use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use composer_protocol::{ClientCommand, ClientRequest, CommandResult, ServerMessage};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::Endpoint;

/// What the connection reports: (re)connected, lost, or a message from the server (every event,
/// and the replies to requests it didn't make through [`Connection::request`]).
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "nearly every event is a message; boxing it would allocate per message for nothing"
)]
pub enum Event {
    Connected,
    Disconnected(String),
    Message(ServerMessage),
}

/// Ids of [`Connection::request`]s start here, clear of a caller's own ids.
const OWN_IDS: u64 = 1 << 40;
const BACKOFF_FIRST: Duration = Duration::from_millis(500);
const BACKOFF_MAX: Duration = Duration::from_secs(10);

type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<CommandResult, String>>>>>;

/// A workspace's WebSocket, kept open: it reconnects (backing off up to 10 s), pairs
/// [`Connection::request`]s with their replies, and reports everything else as [`Event`]s.
#[derive(Clone)]
pub struct Connection {
    outgoing: mpsc::UnboundedSender<ClientRequest>,
    waiting: Waiting,
    next: Arc<AtomicU64>,
    connected: Arc<AtomicBool>,
}

impl Connection {
    /// Connects to `endpoint` (and keeps reconnecting); its events.
    pub fn open(endpoint: Endpoint) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (outgoing, mut requests) = mpsc::unbounded_channel::<ClientRequest>();
        let (events, received) = mpsc::unbounded_channel::<Event>();
        let waiting: Waiting = Arc::default();
        let connected = Arc::new(AtomicBool::new(false));
        let (task_waiting, task_connected) = (waiting.clone(), connected.clone());
        tokio::spawn(async move {
            let mut backoff = BACKOFF_FIRST;
            loop {
                if let Ok((socket, _)) = tokio_tungstenite::connect_async(endpoint.ws_url()).await {
                    backoff = BACKOFF_FIRST;
                    task_connected.store(true, Ordering::SeqCst);
                    while requests.try_recv().is_ok() {} // from before: stale
                    if events.send(Event::Connected).is_err() {
                        return;
                    }
                    let reason = pump(socket, &mut requests, &events, &task_waiting).await;
                    task_connected.store(false, Ordering::SeqCst);
                    fail_waiting(&task_waiting, &reason);
                    if events.send(Event::Disconnected(reason)).is_err() {
                        return;
                    }
                } else if events
                    .send(Event::Disconnected("can't connect".into()))
                    .is_err()
                {
                    return;
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        });
        let connection = Self {
            outgoing,
            waiting,
            next: Arc::new(AtomicU64::new(OWN_IDS)),
            connected,
        };
        (connection, received)
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Sends a request with the caller's own id; its reply comes as an [`Event::Message`].
    pub fn send(&self, request: ClientRequest) -> Result<(), String> {
        if !self.is_connected() {
            return Err("not connected".into());
        }
        self.outgoing
            .send(request)
            .map_err(|_| "the connection is closed".to_string())
    }

    /// Sends `command` and waits for its reply (an error reply is the `Err`).
    pub async fn request(&self, command: ClientCommand) -> Result<CommandResult, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (reply, answer) = oneshot::channel();
        self.waiting.lock().expect("waiting lock").insert(id, reply);
        if let Err(e) = self.send(ClientRequest { id, command }) {
            self.waiting.lock().expect("waiting lock").remove(&id);
            return Err(e);
        }
        answer
            .await
            .map_err(|_| "the connection is closed".to_string())?
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Moves requests out and messages in until the socket goes; why it went.
async fn pump(
    socket: Socket,
    requests: &mut mpsc::UnboundedReceiver<ClientRequest>,
    events: &mpsc::UnboundedSender<Event>,
    waiting: &Waiting,
) -> String {
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            request = requests.recv() => {
                let Some(request) = request else { return "closed".into() };
                let json = serde_json::to_string(&request).expect("requests serialize");
                if let Err(e) = sink.send(Message::Text(json.into())).await {
                    return e.to_string();
                }
            }
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    let Ok(message) = serde_json::from_str::<ServerMessage>(&text) else { continue };
                    if let Some(message) = answer(waiting, message) {
                        let _ = events.send(Event::Message(message));
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return e.to_string(),
                None => return "connection closed".into(),
            }
        }
    }
}

/// A reply to one of [`Connection::request`]'s goes to it (`None`); anything else is an event.
fn answer(waiting: &Waiting, message: ServerMessage) -> Option<ServerMessage> {
    let id = match &message {
        ServerMessage::Ok { id, .. } | ServerMessage::Error { id: Some(id), .. } => *id,
        _ => return Some(message),
    };
    let Some(reply) = waiting.lock().expect("waiting lock").remove(&id) else {
        return Some(message);
    };
    let _ = reply.send(match message {
        ServerMessage::Ok { result, .. } => Ok(result),
        ServerMessage::Error { message, .. } => Err(message),
        _ => unreachable!("only replies get here"),
    });
    None
}

fn fail_waiting(waiting: &Waiting, reason: &str) {
    for (_, reply) in waiting.lock().expect("waiting lock").drain() {
        let _ = reply.send(Err(format!("disconnected: {reason}")));
    }
}
