use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use composer_protocol::{RunKind, SessionId, SessionInfo, TurnState};

use crate::error::{Result, ServerError};
use crate::session::{Session, SessionFolder};

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

    /// A new session of `kind` in `site`.
    pub fn create(
        &self,
        kind: RunKind,
        model: Option<String>,
        site: Option<String>,
    ) -> Result<Arc<Session>> {
        self.create_run(kind, model, site, None, None, None)
    }

    /// A new session of `kind` in `site`, following the workflow `folder` there if given,
    /// spawned by `parent` if given (a webhook's stored run, `spawn_run`), and run by the outside
    /// agent `harness` if given (checked by the caller).
    pub fn create_run(
        &self,
        kind: RunKind,
        model: Option<String>,
        site: Option<String>,
        folder: Option<String>,
        parent: Option<SessionId>,
        harness: Option<String>,
    ) -> Result<Arc<Session>> {
        let mut sessions = self.sessions.write().expect("sessions lock");
        let id = loop {
            let id = format!("{:04x}", rand::random::<u16>());
            if !sessions.contains_key(&id) {
                break id;
            }
        };
        let info = SessionInfo {
            id: id.clone(),
            kind,
            site,
            folder,
            parent,
            harness,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            model,
            preview: None,
            updated: chrono::Utc::now().to_rfc3339(),
        };
        let folder = SessionFolder::of(&self.dir, &id).map_err(ServerError::Invalid)?;
        let session = Session::create(folder.path(), info)?;
        sessions.insert(id, session.clone());
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

    /// Forgets session `id` and deletes its folder.
    pub fn remove(&self, id: &str) -> Result<Arc<Session>> {
        let session = self
            .sessions
            .write()
            .expect("sessions lock")
            .remove(id)
            .ok_or_else(|| ServerError::SessionNotFound(id.to_string()))?;
        std::fs::remove_dir_all(session.dir()).map_err(|e| ServerError::io(session.dir(), e))?;
        Ok(session)
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
