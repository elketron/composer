use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use composer_protocol::{ChatMessage, Seq, ServerMessage, SessionInfo, TurnState};
use tokio_util::sync::CancellationToken;

use crate::error::{Result, ServerError};
use crate::hub::EventHub;
use crate::provider::ModelMessage;
use crate::session::{History, Transcript};

/// One conversation with an agent. Stored in `state/sessions/<id>/`:
/// `meta.json` (info), `events.jsonl` (transcript), `history.jsonl` (model messages).
#[derive(Debug)]
pub struct Session {
    id: String,
    dir: PathBuf,
    info: Mutex<SessionInfo>,
    transcript: Transcript,
    history: Mutex<History>,
    /// User messages waiting for the next turn.
    inbox: Mutex<VecDeque<ChatMessage>>,
    running: AtomicBool,
    cancel: Mutex<CancellationToken>,
}

impl Session {
    pub fn create(dir: &Path, info: SessionInfo) -> Result<Arc<Self>> {
        std::fs::create_dir_all(dir).map_err(|e| ServerError::io(dir, e))?;
        let session = Self::open(dir, info)?;
        session.save_meta();
        Ok(session)
    }

    /// Loads a stored session. A turn interrupted by a restart is left idle.
    pub fn load(dir: &Path) -> Result<Arc<Self>> {
        let meta = dir.join("meta.json");
        let text = std::fs::read_to_string(&meta).map_err(|e| ServerError::io(&meta, e))?;
        let mut info: SessionInfo = serde_json::from_str(&text)
            .map_err(|e| ServerError::Invalid(format!("{}: {e}", meta.display())))?;
        info.state = TurnState::Idle;
        Self::open(dir, info)
    }

    fn open(dir: &Path, info: SessionInfo) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            id: info.id.clone(),
            dir: dir.to_path_buf(),
            transcript: Transcript::open(&dir.join("events.jsonl"), info.last_seq)?,
            history: Mutex::new(History::load(&dir.join("history.jsonl"))?),
            info: Mutex::new(info),
            inbox: Mutex::new(VecDeque::new()),
            running: AtomicBool::new(false),
            cancel: Mutex::new(CancellationToken::new()),
        }))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn info(&self) -> SessionInfo {
        self.info.lock().expect("info lock").clone()
    }

    /// Changes the info, saves it and broadcasts `session.updated`.
    pub fn update_info(&self, hub: &EventHub, change: impl FnOnce(&mut SessionInfo)) {
        let info = {
            let mut info = self.info.lock().expect("info lock");
            change(&mut info);
            info.last_seq = self.transcript.last_seq();
            info.updated = chrono::Utc::now().to_rfc3339();
            info.clone()
        };
        self.save_meta();
        hub.send(ServerMessage::SessionUpdated { session: info });
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    pub fn emit(&self, hub: &EventHub, make: impl FnOnce(Seq) -> ServerMessage) -> Seq {
        self.transcript.emit(hub, make)
    }

    pub fn push_history(&self, message: ModelMessage) -> Result<()> {
        self.history.lock().expect("history lock").push(message)
    }

    pub fn history(&self) -> Vec<ModelMessage> {
        self.history
            .lock()
            .expect("history lock")
            .messages()
            .to_vec()
    }

    pub fn replace_history(&self, messages: Vec<ModelMessage>) -> Result<()> {
        self.history.lock().expect("history lock").replace(messages)
    }

    /// Marks the session busy if it is idle (for work outside a turn, like compaction).
    pub fn try_start(&self) -> bool {
        !self.running.swap(true, Ordering::SeqCst)
    }

    /// Queues a message. Returns `true` if this caller should start the turn
    /// loop, `false` if one is already running (it will pick the message up).
    pub fn enqueue(&self, message: ChatMessage) -> bool {
        self.inbox.lock().expect("inbox lock").push_back(message);
        !self.running.swap(true, Ordering::SeqCst)
    }

    pub fn take_inbox(&self) -> Vec<ChatMessage> {
        self.inbox.lock().expect("inbox lock").drain(..).collect()
    }

    /// Called when the turn loop runs out of messages. Returns `true` if a
    /// message arrived in the meantime and the loop must continue.
    pub fn stop_running(&self) -> bool {
        self.running.store(false, Ordering::SeqCst);
        let pending = !self.inbox.lock().expect("inbox lock").is_empty();
        pending && !self.running.swap(true, Ordering::SeqCst)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// A fresh cancellation token for a new turn.
    pub fn new_turn_token(&self) -> CancellationToken {
        let token = CancellationToken::new();
        *self.cancel.lock().expect("cancel lock") = token.clone();
        token
    }

    pub fn cancel(&self) {
        self.cancel.lock().expect("cancel lock").cancel();
    }

    fn save_meta(&self) {
        let path = self.dir.join("meta.json");
        let json = serde_json::to_string_pretty(&*self.info.lock().expect("info lock"))
            .expect("info serializes");
        if let Err(e) = std::fs::write(&path, json) {
            tracing::error!("writing {}: {e}", path.display());
        }
    }
}
