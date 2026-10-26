use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use composer_protocol::{SessionId, SessionInfo, TurnState};

use crate::error::{Result, ServerError};
use crate::session::Session;

/// All sessions, loaded from `state/sessions/` at startup.
#[derive(Debug)]
pub struct SessionManager {
    dir: PathBuf,
    sessions: RwLock<HashMap<SessionId, Arc<Session>>>,
}

impl SessionManager {
    pub fn load(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|e| ServerError::io(dir, e))?;
        let mut sessions = HashMap::new();
        for entry in std::fs::read_dir(dir)
            .map_err(|e| ServerError::io(dir, e))?
            .flatten()
        {
            if !entry.path().is_dir() {
                continue;
            }
            match Session::load(&entry.path()) {
                Ok(session) => {
                    sessions.insert(session.id().to_string(), session);
                }
                Err(e) => tracing::warn!("skipping session {}: {e}", entry.path().display()),
            }
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            sessions: RwLock::new(sessions),
        })
    }

    pub fn create(&self, agent: &str, project: Option<String>) -> Result<Arc<Session>> {
        let mut sessions = self.sessions.write().expect("sessions lock");
        let id = loop {
            let id = format!("{:04x}", rand::random::<u16>());
            if !sessions.contains_key(&id) {
                break id;
            }
        };
        let info = SessionInfo {
            id: id.clone(),
            agent: agent.to_string(),
            project,
            ticket: None,
            parent: None,
            worker: None,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            model: None,
            preview: None,
            updated: chrono::Utc::now().to_rfc3339(),
        };
        let session = Session::create(&self.dir.join(&id), info)?;
        sessions.insert(id, session.clone());
        Ok(session)
    }

    /// Creates a session with a given id and info (a worker mirrors its parent's session id).
    pub fn create_with_info(&self, info: SessionInfo) -> Result<Arc<Session>> {
        let mut sessions = self.sessions.write().expect("sessions lock");
        let session = Session::create(&self.dir.join(&info.id), info)?;
        sessions.insert(session.id().to_string(), session.clone());
        Ok(session)
    }

    pub fn get(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions
            .read()
            .expect("sessions lock")
            .get(id)
            .cloned()
            .ok_or_else(|| ServerError::SessionNotFound(id.to_string()))
    }

    /// Most recently updated first.
    pub fn list(&self) -> Vec<SessionInfo> {
        let mut infos: Vec<_> = self
            .sessions
            .read()
            .expect("sessions lock")
            .values()
            .map(|s| s.info())
            .collect();
        infos.sort_by(|a, b| b.updated.cmp(&a.updated));
        infos
    }
}
