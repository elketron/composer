use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use composer_core::{Index, Page, SiteUrl, Workspace};
use composer_protocol::{ApprovalDecision, ChangedBy, PageChangeKind, ServerMessage, TurnState};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::actions::{Approvals, PendingApprovals};
use crate::config::Config;
use crate::devices::Devices;
use crate::error::{Result, ServerError};
use crate::hash::content_hash;
use crate::home::Home;
use crate::hub::EventHub;
use crate::init::local_token;
use crate::provider::{OpenAiProvider, Provider};
use crate::session::{Session, SessionManager};
use crate::slot_pool::SlotPool;
use crate::tools::ToolRegistry;
use crate::worker::{ParentLink, WorkerRegistry};

/// The running harness. Shared as `Arc<Server>` by the API, turns and tools.
pub struct Server {
    pub home: Home,
    pub config: Config,
    pub workspace: Workspace,
    pub index: RwLock<Index>,
    pub provider: Arc<dyn Provider>,
    pub slots: Arc<SlotPool>,
    pub hub: EventHub,
    pub sessions: SessionManager,
    pub tools: ToolRegistry,
    pub approvals: Approvals,
    pub pending_approvals: PendingApprovals,
    /// Required on every request (header or `?token=`).
    pub token: String,
    /// Paired devices (remote access), each with its own token.
    pub devices: Devices,
    /// Worker processes this server started.
    pub workers: WorkerRegistry,
    /// The binary started for workers (`$COMPOSER_WORKER_EXE`, else this executable).
    pub worker_exe: PathBuf,
    /// Set in a worker process: the server that spawned it.
    pub parent: Option<Arc<ParentLink>>,
    /// Worker processes: the agent called `finish`.
    worker_finished: AtomicBool,
    listen_addr: OnceLock<SocketAddr>,
    /// Hash of each file as we last saw or wrote it; tells the watcher which changes are ours.
    known_hashes: Mutex<HashMap<PathBuf, String>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Server {
    /// Loads config from the home folder and connects to the configured provider.
    pub async fn start(home: Home) -> Result<Arc<Self>> {
        let config = Config::load(&home.config_file())?;
        let provider = OpenAiProvider::connect(&config.provider)
            .await
            .map_err(ServerError::Provider)?;
        tracing::info!("model: {}", provider.model());
        Self::new(home, config, Arc::new(provider))
    }

    pub fn new(home: Home, config: Config, provider: Arc<dyn Provider>) -> Result<Arc<Self>> {
        Self::build(home, config, provider, None)
    }

    /// The server inside a worker process: no API, slots and approvals come from the
    /// parent, and its sessions live in `state/workers/sessions` (the parent keeps the real transcript).
    pub fn new_worker(
        home: Home,
        config: Config,
        provider: Arc<dyn Provider>,
        parent: Arc<ParentLink>,
    ) -> Result<Arc<Self>> {
        Self::build(home, config, provider, Some(parent))
    }

    fn build(
        home: Home,
        config: Config,
        provider: Arc<dyn Provider>,
        parent: Option<Arc<ParentLink>>,
    ) -> Result<Arc<Self>> {
        let workspace = Workspace::new(home.sites());
        if !workspace.root().is_dir() {
            return Err(ServerError::Invalid(format!(
                "{} does not exist; run `composer init` first",
                workspace.root().display()
            )));
        }
        let index = Index::build(&workspace)?;
        tracing::info!("indexed {} pages", index.len());
        let hub = EventHub::new(4096);
        let (sessions_dir, slots, approvals) = match &parent {
            None => (
                home.sessions(),
                SlotPool::new(config.agent.slots, hub.clone()),
                Approvals::load(&home.approvals_file())?,
            ),
            Some(link) => (
                home.state().join("workers").join("sessions"),
                SlotPool::remote(link.clone(), hub.clone()),
                Approvals::load_read_only(&home.approvals_file())?,
            ),
        };
        let worker_exe = std::env::var_os("COMPOSER_WORKER_EXE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .unwrap_or_else(|| PathBuf::from("composer"));
        Ok(Arc::new(Self {
            token: local_token(&home)?,
            sessions: SessionManager::load(&sessions_dir)?,
            slots,
            tools: ToolRegistry::standard(),
            approvals,
            pending_approvals: PendingApprovals::default(),
            workers: WorkerRegistry::default(),
            devices: Devices::new(&home.state()),
            worker_exe,
            parent,
            worker_finished: AtomicBool::new(false),
            listen_addr: OnceLock::new(),
            index: RwLock::new(index),
            known_hashes: Mutex::new(HashMap::new()),
            watcher: Mutex::new(None),
            workspace,
            provider,
            hub,
            config,
            home,
        }))
    }

    /// The local token, or a paired device's token.
    pub fn accepts_token(&self, token: &str) -> bool {
        token == self.token || self.devices.verify(token).is_some()
    }

    /// `server.name` from config.toml, else the hostname.
    pub fn name(&self) -> String {
        if !self.config.server.name.trim().is_empty() {
            return self.config.server.name.trim().to_string();
        }
        std::fs::read_to_string("/etc/hostname")
            .map(|h| h.trim().to_string())
            .ok()
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "composer".into())
    }

    /// Where the API listens (set by `serve`); workers connect back here.
    pub fn listen_addr(&self) -> Option<SocketAddr> {
        self.listen_addr.get().copied()
    }

    /// Worker processes: the agent called `finish`.
    pub fn worker_finished(&self) -> bool {
        self.worker_finished.load(Ordering::SeqCst)
    }

    pub fn mark_worker_finished(&self) {
        self.worker_finished.store(true, Ordering::SeqCst);
    }

    /// Watches the workspace and serves the API until the listener fails.
    pub async fn serve(self: &Arc<Self>, listener: TcpListener) -> Result<()> {
        if let Ok(addr) = listener.local_addr() {
            let _ = self.listen_addr.set(addr);
        }
        *self.watcher.lock().expect("watcher lock") = Some(crate::watcher::watch(self)?);
        let router = crate::api::router(self.clone());
        axum::serve(listener, router)
            .await
            .map_err(|e| ServerError::Invalid(format!("server stopped: {e}")))
    }

    /// Writes a page, updates the index and broadcasts `page.changed`.
    /// Returns the canonical URL and whether the page was created.
    pub fn write_page(
        &self,
        url: &SiteUrl,
        content: &str,
        by: ChangedBy,
    ) -> Result<(SiteUrl, bool)> {
        self.write_file(url, content.as_bytes(), by)
    }

    /// Like [`Server::write_page`] for any file; markdown files are indexed.
    pub fn write_file(
        &self,
        url: &SiteUrl,
        content: &[u8],
        by: ChangedBy,
    ) -> Result<(SiteUrl, bool)> {
        let created = self.workspace.locate(url).is_err();
        let written = self.workspace.write_file(url, content)?;
        let path = self.workspace.locate(&written)?;
        self.known_hashes
            .lock()
            .expect("hashes lock")
            .insert(path, content_hash(content));
        self.index_page(&written, &String::from_utf8_lossy(content));
        let kind = if created {
            PageChangeKind::Created
        } else {
            PageChangeKind::Modified
        };
        self.hub.send(ServerMessage::PageChanged {
            url: written.to_string(),
            kind,
            by,
        });
        Ok((written, created))
    }

    /// Syncs one path after a filesystem event. Ignores changes we made ourselves.
    pub fn sync_path(&self, path: &Path) {
        if !is_watched(self.workspace.root(), path) || path.is_dir() {
            return;
        }
        let Some(url) = self.workspace.url_for_path(path) else {
            return;
        };
        let is_page = path.extension().is_some_and(|e| e == "md");
        let mut hashes = self.known_hashes.lock().expect("hashes lock");
        let kind = match std::fs::read(path) {
            Ok(bytes) => {
                let hash = content_hash(&bytes);
                let previous = hashes.insert(path.to_path_buf(), hash.clone());
                if previous.as_ref() == Some(&hash) {
                    return;
                }
                let indexed = is_page && self.index.read().expect("index lock").get(&url).is_some();
                if is_page {
                    self.index_page(&url, &String::from_utf8_lossy(&bytes));
                }
                if previous.is_some() || indexed {
                    PageChangeKind::Modified
                } else {
                    PageChangeKind::Created
                }
            }
            Err(_) => {
                hashes.remove(path);
                if is_page {
                    self.index.write().expect("index lock").remove(&url);
                }
                PageChangeKind::Deleted
            }
        };
        drop(hashes);
        self.hub.send(ServerMessage::PageChanged {
            url: url.to_string(),
            kind,
            by: ChangedBy::External,
        });
    }

    /// Asks attached clients to approve a recipe version and waits for the answer.
    /// The session shows `needs approval` meanwhile. `None` if the turn was cancelled.
    #[allow(clippy::too_many_arguments)]
    pub async fn request_approval(
        &self,
        session: &Session,
        site: &str,
        recipe: &str,
        args: &[String],
        body: String,
        hash: &str,
        cancel: &CancellationToken,
    ) -> Option<ApprovalDecision> {
        // In a worker process the parent asks its clients.
        if let Some(link) = &self.parent {
            session.update_info(&self.hub, |info| info.state = TurnState::AwaitingApproval);
            let answer = tokio::select! {
                answer = link.ask_approval(site, recipe, args, body, hash) => answer.ok(),
                _ = cancel.cancelled() => None,
            };
            session.update_info(&self.hub, |info| info.state = TurnState::Running);
            return answer.map(|approved| {
                if approved {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Deny
                }
            });
        }
        let request = format!("{:08x}", rand::random::<u32>());
        let message = ServerMessage::ApprovalRequest {
            request: request.clone(),
            session: session.id().to_string(),
            site: site.to_string(),
            recipe: recipe.to_string(),
            args: args.to_vec(),
            body,
            hash: hash.to_string(),
        };
        let answer = self.pending_approvals.add(&request, message.clone());
        session.update_info(&self.hub, |info| info.state = TurnState::AwaitingApproval);
        self.hub.send(message);
        let decision = tokio::select! {
            decision = answer => decision.ok(),
            _ = cancel.cancelled() => None,
        };
        self.pending_approvals.remove(&request);
        self.hub.send(ServerMessage::ApprovalResolved {
            request,
            session: session.id().to_string(),
            approved: decision == Some(ApprovalDecision::Approve),
        });
        session.update_info(&self.hub, |info| info.state = TurnState::Running);
        decision
    }

    /// Hash of a page as last seen by the server, reading it if unknown.
    pub fn page_hash(&self, path: &Path) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        Some(content_hash(&bytes))
    }

    fn index_page(&self, url: &SiteUrl, content: &str) {
        if url.path().ends_with(".md") {
            let page = Page::parse(url.clone(), content.to_string());
            self.index.write().expect("index lock").upsert(&page);
        }
    }
}

/// Inside a site, not in a hidden folder, `node_modules` or a site's `ui/`.
fn is_watched(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let parts: Vec<_> = rel.iter().filter_map(|p| p.to_str()).collect();
    parts.len() >= 2
        && !parts
            .iter()
            .any(|p| p.starts_with('.') || *p == "node_modules")
        && !(parts.len() > 2 && parts[1] == "ui")
}
