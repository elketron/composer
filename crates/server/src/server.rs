use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use composer_core::{Index, Page, SiteUrl, Web};
use composer_protocol::{
    ApprovalDecision, ApprovalKind, ChangedBy, PageChangeKind, ServerMessage, SessionInfo,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::actions::{ApprovalWait, Approvals, PendingApprovals};
use crate::config::Config;
use crate::devices::Devices;
use crate::error::{Result, ServerError};
use crate::harness::ToolRules;
use crate::hash::content_hash;
use crate::home::Home;
use crate::hub::EventHub;
use crate::init::local_token;
use crate::mcp_clients::McpServers;
use crate::provider::{OpenAiProvider, Provider};
use crate::session::{Session, SessionManager};
use crate::slot_pool::SlotPool;
use crate::tools::ToolRegistry;
use crate::watches::{WRITER, WatchRegistry};

/// The running harness. Shared as `Arc<Server>` by the API, turns and tools.
pub struct Server {
    pub home: Home,
    /// `config.toml`, swapped when it changes ([`Server::reload_config`]); read it with [`Server::config`].
    config: RwLock<Arc<Config>>,
    pub web: Web,
    pub index: RwLock<Index>,
    pub provider: Arc<dyn Provider>,
    pub slots: Arc<SlotPool>,
    pub hub: EventHub,
    pub sessions: SessionManager,
    pub tools: ToolRegistry,
    /// MCP servers from `config.toml`, their tools for the agents that list them.
    mcp: RwLock<Arc<McpServers>>,
    pub approvals: Approvals,
    pub pending_approvals: PendingApprovals,
    /// Required on every request (header or `?token=`).
    pub token: String,
    /// Paired devices (remote access), each with its own token.
    pub devices: Devices,
    /// Browser sessions: what the auth cookie holds, never a token.
    pub web_sessions: crate::api::WebSessions,
    /// Its sessions' terminals (ticket 0176).
    pub terminals: crate::terminals::Terminals,
    /// Sessions' watches on site files (`state/watches.json`).
    pub watches: WatchRegistry,
    /// Tools the user said `always` to, per folder.
    pub tool_rules: ToolRules,
    listen_addr: OnceLock<SocketAddr>,
    /// Hash of each file as we last saw or wrote it; tells the watcher which changes are ours.
    known_hashes: Mutex<HashMap<PathBuf, String>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Server {
    /// Loads config from the home folder and connects to the configured provider.
    pub async fn start(home: Home) -> Result<Arc<Self>> {
        let config = home.load_config()?;
        let provider = OpenAiProvider::connect(&config.provider)
            .await
            .map_err(ServerError::Provider)?;
        tracing::info!("model: {}", provider.model());
        Self::new(home, config, Arc::new(provider))
    }

    pub fn new(home: Home, config: Config, provider: Arc<dyn Provider>) -> Result<Arc<Self>> {
        crate::viewer_upstream::ViewerUpstream::warn_if_refused(&config);
        // Scoped mounts (ticket 0122): a symlink inside a site may point into a site's folder;
        // one that lands elsewhere is refused, not followed.
        let web = Web::with_allowed(home.sites(), vec![]);
        web.set_allowed(site_folders(&web));
        if !web.root().is_dir() {
            return Err(ServerError::Invalid(format!(
                "{} does not exist; run `composer init` first",
                web.root().display()
            )));
        }
        let index = Index::build(&web)?;
        tracing::info!("indexed {} pages", index.len());
        let hub = EventHub::new(4096);
        Ok(Arc::new(Self {
            token: local_token(&home)?,
            sessions: SessionManager::load(&home.sessions())?,
            slots: SlotPool::new(config.agent.slots, hub.clone()),
            tools: ToolRegistry::standard(),
            mcp: RwLock::new(Arc::new(McpServers::new(
                &config.mcp,
                home.env()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                home.sites(),
            ))),
            approvals: Approvals::load(&home.approvals_file())?,
            pending_approvals: PendingApprovals::default(),
            web_sessions: crate::api::WebSessions::default(),
            terminals: crate::terminals::Terminals::default(),
            watches: WatchRegistry::load(home.state().join("watches.json")),
            tool_rules: ToolRules::load(home.state().join("tool_rules.json")),
            devices: Devices::new(&home.state()),
            listen_addr: OnceLock::new(),
            index: RwLock::new(index),
            known_hashes: Mutex::new(HashMap::new()),
            watcher: Mutex::new(None),
            web,
            provider,
            hub,
            config: RwLock::new(Arc::new(config)),
            home,
        }))
    }

    /// The config as it is now; a reload swaps it, so don't keep it across awaits for long.
    pub fn config(&self) -> Arc<Config> {
        self.config.read().expect("config lock").clone()
    }

    /// The MCP servers of the current config.
    pub fn mcp(&self) -> Arc<McpServers> {
        self.mcp.read().expect("mcp lock").clone()
    }

    /// Reads `config.toml` again and runs with it from now on. Returns `None` when nothing
    /// changed, else the changed settings that still need a restart. A file that doesn't parse
    /// is an error and the old config stays. Running turns and workers keep what they started with.
    pub fn reload_config(&self) -> Result<Option<Vec<String>>> {
        let next = self.home.load_config()?;
        let mut current = self.config.write().expect("config lock");
        if **current == next {
            return Ok(None);
        }
        let restart_needed = [
            ("provider", current.provider != next.provider),
            ("server.bind", current.server.bind != next.server.bind),
            ("agent.slots", current.agent.slots != next.agent.slots),
            (
                "attachments.max_mb",
                current.attachments.max_mb != next.attachments.max_mb,
            ),
        ]
        .into_iter()
        .filter(|(_, changed)| *changed)
        .map(|(name, _)| name.to_string())
        .collect();
        if current.mcp != next.mcp {
            let mut mcp = self.mcp.write().expect("mcp lock");
            *mcp = Arc::new(mcp.reconfigured(&next.mcp));
        }
        crate::viewer_upstream::ViewerUpstream::warn_if_refused(&next);
        *current = Arc::new(next);
        tracing::info!("config.toml reloaded");
        Ok(Some(restart_needed))
    }

    /// [`Server::reload_config`] after the file changed, told to every client.
    pub fn config_file_changed(&self) {
        let message = match self.reload_config() {
            Ok(None) => return,
            Ok(Some(restart_needed)) => ServerMessage::ConfigReloaded {
                restart_needed,
                error: None,
            },
            Err(e) => {
                tracing::warn!("config.toml: {e}; keeping the running config");
                ServerMessage::ConfigReloaded {
                    restart_needed: vec![],
                    error: Some(e.to_string()),
                }
            }
        };
        self.hub.send(message);
    }

    /// The local token, a paired device's token, or a browser session one of them opened.
    pub fn accepts_token(&self, token: &str) -> bool {
        self.credential(token).is_some()
    }

    /// What kind of credential `token` is, `None` when it is none: the local token, a paired
    /// device's token, or a browser session, which is the kind of the credential that opened it
    /// (and nothing once that device is revoked).
    pub fn credential(&self, token: &str) -> Option<crate::api::Credential> {
        use crate::api::{Credential, WebSessions};
        if token.starts_with(WebSessions::PREFIX) {
            return self
                .web_sessions
                .credential(token, |minter| self.credential_alive(minter));
        }
        if crate::secret::secret_eq(token, &self.token) {
            return Some(Credential::Local);
        }
        self.devices.find(token).map(|device| Credential::Device {
            name: device.name,
            token_hash: device.token_hash,
        })
    }

    /// Whether `credential` still counts: a device's only while that device is paired.
    pub fn credential_alive(&self, credential: &crate::api::Credential) -> bool {
        match credential {
            crate::api::Credential::Local => true,
            crate::api::Credential::Device { token_hash, .. } => self.devices.is_paired(token_hash),
        }
    }

    /// `server.name` from config.toml, else the hostname.
    pub fn name(&self) -> String {
        let config = self.config();
        if !config.server.name.trim().is_empty() {
            return config.server.name.trim().to_string();
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

    /// `session` as a run's sink.
    pub fn sink(&self, session: &Arc<Session>) -> Arc<dyn crate::session::RunSink> {
        Arc::new(crate::session::SessionSink::new(
            session.clone(),
            self.hub.clone(),
        ))
    }

    /// The folder of the site `session` is in (`folder:`), when it names one that exists.
    pub fn session_folder(&self, session: &SessionInfo) -> Option<PathBuf> {
        let site = crate::agent::Site::load(&self.web, session.site.as_deref()?).ok()?;
        site.existing_folder().ok()
    }

    /// Builds the index again from the web as it is now (a mount closed, a site's folder
    /// changed), and the folders mounts may point into.
    pub fn reindex(&self) {
        self.web.set_allowed(site_folders(&self.web));
        match Index::build(&self.web) {
            Ok(index) => *self.index.write().expect("index lock") = index,
            Err(e) => tracing::warn!("reindexing: {e}"),
        }
    }

    /// Starts what runs whether or not anyone is connected: the file watcher and watches.
    /// `listen` is where its API is served.
    pub fn start_background(self: &Arc<Self>, listen: SocketAddr) -> Result<()> {
        let _ = self.listen_addr.set(listen);
        *self.watcher.lock().expect("watcher lock") = Some(crate::watcher::watch(self)?);
        tokio::spawn(crate::watches::run_watches(self.clone()));
        Ok(())
    }

    /// Watches the home and serves its API until the listener fails or a signal stops it.
    pub async fn serve(self: &Arc<Self>, listener: TcpListener) -> Result<()> {
        let addr = listener
            .local_addr()
            .map_err(|e| ServerError::Invalid(format!("listener address: {e}")))?;
        self.start_background(addr)?;
        let router = crate::api::router(self.clone());
        let serving = axum::serve(listener, router);
        let result = tokio::select! {
            served = serving => {
                served.map_err(|e| ServerError::Invalid(format!("server stopped: {e}")))
            }
            _ = shutdown_signal() => {
                tracing::info!("shutting down");
                Ok(())
            }
        };
        // Nothing a session started outlives the server: background processes, shells.
        let server = self.clone();
        let _ = tokio::task::spawn_blocking(move || server.terminals.shutdown()).await;
        result
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
        let created = self.web.locate(url).is_err();
        let written = self.web.write_file(url, content)?;
        let path = self.web.locate(&written)?;
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
        let writer = WRITER.try_with(|session| session.clone()).ok();
        self.watches
            .changed(&written.to_string(), kind, writer.as_deref());
        Ok((written, created))
    }

    /// Syncs one path after a filesystem event. Ignores changes we made ourselves.
    pub fn sync_path(&self, path: &Path) {
        if !is_watched(self.web.root(), path) || !self.web.allows(path) {
            return;
        }
        if path.is_dir() {
            // A folder created since the watch started: the event arrives on the parent's
            // watch, but files written into the new folder before the watcher attaches to it
            // generate none — rescan it so they still reach the index (ticket 0071).
            self.rescan_dir(path);
            return;
        }
        let Some(url) = self.web.url_for_path(path) else {
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
        self.watches.changed(&url.to_string(), kind, None);
    }

    /// Asks attached clients to approve a recipe version and waits for the answer.
    /// The session shows `needs approval` meanwhile. `None` if the turn was cancelled.
    #[allow(clippy::too_many_arguments)]
    pub async fn request_approval(
        self: &Arc<Self>,
        session: &Session,
        site: &str,
        recipe: &str,
        args: &[String],
        body: String,
        hash: &str,
        cancel: &CancellationToken,
    ) -> Option<ApprovalDecision> {
        self.ask_clients(
            session,
            |request| ServerMessage::ApprovalRequest {
                request,
                session: session.id().to_string(),
                kind: ApprovalKind::Recipe,
                title: String::new(),
                site: site.to_string(),
                recipe: recipe.to_string(),
                args: args.to_vec(),
                body,
                hash: hash.to_string(),
                rule: None,
            },
            cancel,
        )
        .await
    }

    /// Asks this server's clients to approve something other than a recipe (a terminal input, an
    /// MCP tool) and waits. `None` if `cancel` fired first.
    pub async fn ask_approval(
        self: &Arc<Self>,
        session: &Session,
        kind: ApprovalKind,
        title: String,
        body: String,
        rule: Option<String>,
        cancel: &CancellationToken,
    ) -> Option<ApprovalDecision> {
        self.ask_clients(
            session,
            |request| ServerMessage::ApprovalRequest {
                request,
                session: session.id().to_string(),
                kind,
                title,
                site: String::new(),
                recipe: String::new(),
                args: vec![],
                body,
                hash: String::new(),
                rule,
            },
            cancel,
        )
        .await
    }

    /// Sends the approval request `message` (given its id) to every client and waits for an
    /// answer; the session shows `needs approval` meanwhile.
    async fn ask_clients(
        self: &Arc<Self>,
        session: &Session,
        message: impl FnOnce(String) -> ServerMessage,
        cancel: &CancellationToken,
    ) -> Option<ApprovalDecision> {
        let request = format!("{:08x}", rand::random::<u32>());
        let message = message(request.clone());
        let mut wait = ApprovalWait::new(self, session, request.clone());
        let answer = self.pending_approvals.add(&request, message.clone());
        wait.set_announced();
        self.hub.send(message);
        let decision = tokio::select! {
            decision = answer => decision.ok(),
            _ = cancel.cancelled() => None,
        };
        wait.set_approved(matches!(
            decision,
            Some(ApprovalDecision::Approve | ApprovalDecision::Always)
        ));
        drop(wait);
        decision
    }

    /// Indexes every file in a freshly created folder, as a filesystem event for the folder
    /// itself (each file inside it may have been written before the watcher attached).
    fn rescan_dir(&self, dir: &Path) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            self.sync_path(&entry.path());
        }
    }

    /// A folder inside a site, made since the watch started, that is a symlink into an
    /// allowed root: the watcher should watch it (ticket 0122). The inotify backend does
    /// not report it as a directory when it appears (its own walk would follow it, but the
    /// dynamic attach only fires for `ISDIR` events), so the watcher asks us before syncing.
    pub fn mount_created(&self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(self.web.root()) else {
            return false;
        };
        rel.components().count() >= 2
            && std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
            && path.is_dir()
            && self.web.allows(path)
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
pub(crate) fn is_watched(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let parts: Vec<_> = rel.iter().filter_map(|p| p.to_str()).collect();
    (parts.len() >= 2 || (parts.len() == 1 && path.is_dir()))
        && !parts
            .iter()
            .any(|p| p.starts_with('.') || *p == "node_modules")
        && !(parts.len() > 2 && parts[1] == "ui")
}

/// Ctrl-C, or SIGTERM (systemd, `kill`).
pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                match term.as_mut() {
                    Some(term) => { term.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

/// The folders the sites name (`folder:`): where a mount inside a site may point.
fn site_folders(web: &Web) -> Vec<PathBuf> {
    let Ok(sites) = web.sites() else {
        return Vec::new();
    };
    sites
        .iter()
        .filter_map(|site| crate::agent::Site::load(web, &site.name).ok())
        .filter_map(|site| site.existing_folder().ok())
        .collect()
}
