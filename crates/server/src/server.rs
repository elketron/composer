use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use composer_core::{Index, Page, SiteUrl, Web};
use composer_protocol::{
    ApprovalDecision, ApprovalKind, ChangedBy, PageChangeKind, ServerMessage, TurnState,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::actions::{ApprovalAnswer, ApprovalWait, Approvals, Caller, PendingApprovals};
use crate::config::Config;
use crate::devices::Devices;
use crate::error::{Result, ServerError};
use crate::harness::{HarnessSwitches, ToolRules};
use crate::hash::content_hash;
use crate::hub::EventHub;
use crate::init::local_token;
use crate::mcp_clients::McpServers;
use crate::project::{ProjectOwnership, ProjectPath};
use crate::provider::{OpenAiProvider, Provider};
use crate::session::{Session, SessionManager};
use crate::slot_pool::SlotPool;
use crate::tools::{ToolRegistry, expand_home};
use crate::watches::{WRITER, WatchRegistry};
use crate::worker::{ParentLink, Verdict, WorkerApprovals, WorkerRegistry};
use crate::workspace_home::WorkspaceHome;

/// The running harness. Shared as `Arc<Server>` by the API, turns and tools.
pub struct Server {
    pub home: WorkspaceHome,
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
    /// Workers' approvals their orchestrator decides.
    pub worker_approvals: WorkerApprovals,
    /// Required on every request (header or `?token=`).
    pub token: String,
    /// Paired devices (remote access), each with its own token.
    pub devices: Devices,
    /// Worker processes this server started.
    pub workers: WorkerRegistry,
    /// Sessions' watches on site files (`state/watches.json`).
    pub watches: WatchRegistry,
    /// Harnesses turned off per project at runtime (`state/projects.json`).
    pub switches: HarnessSwitches,
    /// Tools the user always allows an external harness, per project.
    pub tool_rules: ToolRules,
    /// The binary started for workers (`$COMPOSER_WORKER_EXE`, else this executable).
    pub worker_exe: PathBuf,
    /// Set in a worker process: the server that spawned it.
    pub parent: Option<Arc<ParentLink>>,
    /// Worker processes: the agent called `finish`.
    worker_finished: AtomicBool,
    listen_addr: OnceLock<SocketAddr>,
    /// Where this workspace's routes are mounted (`/w/<name>`, or empty when served alone).
    route_base: OnceLock<String>,
    /// Which workspace owns which project: the host's index, shared by all its workspaces (a
    /// worker reads a snapshot of it). Shared with the web's mount guard.
    ownership: Arc<RwLock<Arc<ProjectOwnership>>>,
    /// Hash of each file as we last saw or wrote it; tells the watcher which changes are ours.
    known_hashes: Mutex<HashMap<PathBuf, String>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Server {
    /// Loads config from the home folder and connects to the configured provider.
    pub async fn start(home: WorkspaceHome) -> Result<Arc<Self>> {
        let config = home.load_config()?;
        let provider = OpenAiProvider::connect(&config.provider)
            .await
            .map_err(ServerError::Provider)?;
        tracing::info!("model: {}", provider.model());
        Self::new(home, config, Arc::new(provider))
    }

    pub fn new(
        home: WorkspaceHome,
        config: Config,
        provider: Arc<dyn Provider>,
    ) -> Result<Arc<Self>> {
        Self::build(home, config, provider, None)
    }

    /// The server inside a worker process: no API, slots and approvals come from the
    /// parent, and its sessions live in `state/workers/sessions` (the parent keeps the real transcript).
    pub fn new_worker(
        home: WorkspaceHome,
        config: Config,
        provider: Arc<dyn Provider>,
        parent: Arc<ParentLink>,
    ) -> Result<Arc<Self>> {
        Self::build(home, config, provider, Some(parent))
    }

    fn build(
        home: WorkspaceHome,
        config: Config,
        provider: Arc<dyn Provider>,
        parent: Option<Arc<ParentLink>>,
    ) -> Result<Arc<Self>> {
        // Scoped mounts (ticket 0122): a symlink inside a site may point into the projects
        // directory; one that lands elsewhere is refused, not followed. Same allowance as
        // the launcher scope keys off.
        let web = Web::with_allowed(home.sites(), vec![expand_home(&config.projects.dir)]);
        // Set before the first index, so no page behind a closed mount is ever in it.
        let ownership = Arc::new(RwLock::new(Arc::new(match &parent {
            Some(_) if !home.is_standalone() => ProjectOwnership::snapshot(&home.projects_file()),
            _ => ProjectOwnership::in_memory(),
        })));
        web.set_mount_guard(mount_guard(&home, &ownership));
        if !web.root().is_dir() {
            return Err(ServerError::Invalid(format!(
                "{} does not exist; run `composer init` first",
                web.root().display()
            )));
        }
        let index = Index::build(&web)?;
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
            mcp: RwLock::new(Arc::new(McpServers::new(
                &config.mcp,
                home.env()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ))),
            approvals,
            pending_approvals: PendingApprovals::default(),
            worker_approvals: WorkerApprovals::default(),
            workers: WorkerRegistry::default(),
            watches: match &parent {
                None => WatchRegistry::load(home.state().join("watches.json")),
                Some(_) => WatchRegistry::in_memory(),
            },
            switches: HarnessSwitches::load(home.state().join("projects.json")),
            tool_rules: ToolRules::load(home.state().join("tool_rules.json")),
            devices: Devices::new(&home.state()),
            worker_exe,
            parent,
            worker_finished: AtomicBool::new(false),
            listen_addr: OnceLock::new(),
            route_base: OnceLock::new(),
            ownership,
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
        if current.projects.dir != next.projects.dir {
            // Mounted folders start following the new scope from now on; pages under a mount
            // the new scope drops stay indexed until something changes them.
            self.web.set_allowed(vec![expand_home(&next.projects.dir)]);
        }
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

    /// The local token, or a paired device's token.
    pub fn accepts_token(&self, token: &str) -> bool {
        token == self.token || self.devices.verify(token).is_some()
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

    /// Worker processes: the agent called `finish`.
    pub fn worker_finished(&self) -> bool {
        self.worker_finished.load(Ordering::SeqCst)
    }

    pub fn mark_worker_finished(&self) {
        self.worker_finished.store(true, Ordering::SeqCst);
    }

    /// Joins the host's project ownership index (before serving): from then on a project another
    /// workspace claims leaves this one's index, and its mounts close.
    pub fn share_ownership(self: &Arc<Self>, shared: Arc<ProjectOwnership>) -> Result<()> {
        *self.ownership.write().expect("ownership lock") = shared.clone();
        let (server, workspace) = (Arc::downgrade(self), self.home.name().clone());
        shared.on_claim(Box::new(move |_, owner| {
            if *owner != workspace
                && let Some(server) = server.upgrade()
            {
                server.reindex();
            }
        }));
        self.reindex();
        Ok(())
    }

    fn ownership(&self) -> Arc<ProjectOwnership> {
        self.ownership.read().expect("ownership lock").clone()
    }

    /// The project `path` names (a name in the projects directory, `~/...` or absolute), for
    /// working in it: claimed for this workspace if no workspace owns it yet, refused if another
    /// one does (or owns a folder it is nested in or contains), or if it is part of another
    /// workspace's folder.
    pub fn use_project(&self, path: &str) -> std::result::Result<String, String> {
        let project = self.project(path)?;
        self.ownership().claim(&project, self.home.name())?;
        Ok(project.to_string())
    }

    /// Like [`Server::use_project`] for looking only: refused if another workspace owns it,
    /// nothing claimed.
    pub fn see_project(&self, path: &str) -> std::result::Result<String, String> {
        let project = self.project(path)?;
        self.ownership().check(&project, self.home.name())?;
        Ok(project.to_string())
    }

    /// `path` as a project folder, unless it is part of another workspace's folder.
    fn project(&self, path: &str) -> std::result::Result<ProjectPath, String> {
        let project = ProjectPath::resolve(path, &self.config().projects.dir)?;
        match self.home.foreign_part(project.as_path()) {
            Some(part) => Err(format!(
                "{part}: an agent of workspace `{}` works only inside its own workspace. Pick a \
                 project of this workspace.",
                self.home.name()
            )),
            None => Ok(project),
        }
    }

    /// Whether another workspace owns `project` (for leaving it out of listings).
    pub fn owned_elsewhere(&self, project: &ProjectPath) -> bool {
        self.foreign_path(project.as_path())
    }

    /// Whether `path` (a folder or file) is another workspace's: in or holding its folder, or a
    /// project it owns. What this workspace's agents may not read, link to or be pointed at.
    pub fn foreign_path(&self, path: &Path) -> bool {
        foreign(&self.home, &self.ownership(), path)
    }

    /// Builds the index again from the web as it is now (a mount closed, a project claimed
    /// elsewhere).
    pub fn reindex(&self) {
        match Index::build(&self.web) {
            Ok(index) => *self.index.write().expect("index lock") = index,
            Err(e) => tracing::warn!("reindexing workspace {}: {e}", self.home.name()),
        }
    }

    /// Where this workspace's routes are mounted: `/w/<name>`, or `""` when it is served alone.
    pub fn route_base(&self) -> &str {
        self.route_base.get().map(String::as_str).unwrap_or("")
    }

    /// The URL this workspace's API answers at (`http://<addr>/w/<name>`): what its workers and
    /// MCP bridges connect back to. `None` until it is listening.
    pub fn parent_url(&self) -> Option<String> {
        let addr = self.listen_addr()?;
        Some(format!("http://{addr}{}", self.route_base()))
    }

    /// Starts what runs whether or not anyone is connected: the file watcher, reminders, pipeline
    /// gates, watches, and picking up workers from before a restart. `listen` and `route_base` are
    /// where its API is served (see [`Server::parent_url`]).
    pub fn start_background(self: &Arc<Self>, listen: SocketAddr, route_base: &str) -> Result<()> {
        let _ = self.listen_addr.set(listen);
        let _ = self.route_base.set(route_base.to_string());
        *self.watcher.lock().expect("watcher lock") = Some(crate::watcher::watch(self)?);
        tokio::spawn(crate::reminders::run_reminders(self.clone()));
        tokio::spawn(crate::pipeline::run_gates(self.clone()));
        tokio::spawn(crate::watches::run_watches(self.clone()));
        self.workers.recover_orphans(self);
        self.workers.schedule_paused(self);
        Ok(())
    }

    /// Watches the workspace and serves its API alone (no `/w/<name>` prefix) until the listener
    /// fails. The real server serves every workspace through [`crate::Host`]; tests use this.
    pub async fn serve(self: &Arc<Self>, listener: TcpListener) -> Result<()> {
        let addr = listener
            .local_addr()
            .map_err(|e| ServerError::Invalid(format!("listener address: {e}")))?;
        self.start_background(addr, "")?;
        let router = crate::api::router(self.clone());
        let serving = axum::serve(listener, router);
        tokio::select! {
            served = serving => {
                served.map_err(|e| ServerError::Invalid(format!("server stopped: {e}")))?;
            }
            _ = shutdown_signal() => {
                tracing::info!("shutting down");
            }
        }
        // Workers run in their own process groups: stop them, or they outlive the server.
        self.workers.shutdown().await;
        Ok(())
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
    ///
    /// An app's call (`Caller::App`) goes to the user even in a ticket's session: the app picks the
    /// session, and a prompt can't guard against a mistake if the caller's own side answers it.
    /// A worker process serves no HTTP, so what it passes to its parent is always an agent's.
    #[allow(clippy::too_many_arguments)]
    pub async fn request_approval(
        self: &Arc<Self>,
        session: &Session,
        site: &str,
        recipe: &str,
        args: &[String],
        body: String,
        hash: &str,
        caller: Caller,
        cancel: &CancellationToken,
    ) -> Option<ApprovalAnswer> {
        // In a worker process the parent asks its clients (or this worker's orchestrator).
        if let Some(link) = &self.parent {
            session.update_info(&self.hub, |info| info.state = TurnState::AwaitingApproval);
            let answer = tokio::select! {
                answer = link.ask_approval(site, recipe, args, body, hash) => answer.ok(),
                _ = cancel.cancelled() => None,
            };
            session.update_info(&self.hub, |info| info.state = TurnState::Running);
            return answer.map(|(approved, reason)| ApprovalAnswer {
                decision: if approved {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Deny
                },
                reason,
            });
        }
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
            caller != Caller::App,
            cancel,
        )
        .await
    }

    /// Asks this server's clients to approve something other than a recipe (a ticket merge, an
    /// external harness's tool) and waits. `None` if `cancel` fired first.
    pub async fn ask_approval(
        self: &Arc<Self>,
        session: &Session,
        kind: ApprovalKind,
        title: String,
        body: String,
        rule: Option<String>,
        cancel: &CancellationToken,
    ) -> Option<ApprovalAnswer> {
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
            true,
            cancel,
        )
        .await
    }

    /// Sends the approval request `message` (given its id) to every client and waits for an
    /// answer; the session shows `needs approval` meanwhile. A worker's tool or recipe goes to
    /// its orchestrator first, when it decides them ([`WorkerApprovals`]) and `delegate` allows it.
    async fn ask_clients(
        self: &Arc<Self>,
        session: &Session,
        message: impl FnOnce(String) -> ServerMessage,
        delegate: bool,
        cancel: &CancellationToken,
    ) -> Option<ApprovalAnswer> {
        let request = format!("{:08x}", rand::random::<u32>());
        let mut message = message(request.clone());
        let mut wait = ApprovalWait::new(self, session, request.clone());
        if let ServerMessage::ApprovalRequest {
            kind,
            title,
            body,
            site,
            recipe,
            args,
            ..
        } = &mut message
            && delegate
            && WorkerApprovals::delegable(self, session, *kind)
        {
            let what = match kind {
                ApprovalKind::Recipe => {
                    format!("run recipe `{recipe}` of site://{site} {}", args.join(" "))
                        .trim_end()
                        .to_string()
                }
                _ => title.clone(),
            };
            // `[system1]` first, then the orchestrator (when its agent decides them).
            let mut decided_by = self.config().system1.model.clone();
            let mut verdict = tokio::select! {
                verdict = crate::worker::system1::decide(self, session, &what, body) => verdict,
                _ = cancel.cancelled() => return None,
            };
            if verdict.is_none()
                && let Some(orchestrator) = WorkerApprovals::decider(self, session, *kind)
            {
                decided_by = "Its orchestrator".into();
                verdict = self
                    .worker_approvals
                    .ask(self, &orchestrator, session, &request, &what, body, cancel)
                    .await;
                verdict.as_ref()?;
            }
            let decided = |decision, reason: String| {
                Some(ApprovalAnswer {
                    decision,
                    reason: Some(reason),
                })
            };
            match verdict {
                None => {}
                Some(Verdict::Approve(reason)) => {
                    return decided(ApprovalDecision::Approve, reason);
                }
                Some(Verdict::Deny(reason)) => return decided(ApprovalDecision::Deny, reason),
                Some(Verdict::AskUser(reason)) => {
                    *body = format!("{decided_by} asks you: {reason}\n\n{body}");
                }
            }
        }
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
        decision.map(|decision| ApprovalAnswer {
            decision,
            reason: None,
        })
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

/// What a mount in `home`'s web may not point at: a folder [`foreign`] to its workspace.
fn mount_guard(
    home: &WorkspaceHome,
    ownership: &Arc<RwLock<Arc<ProjectOwnership>>>,
) -> composer_core::MountGuard {
    let (home, ownership) = (home.clone(), ownership.clone());
    Arc::new(move |target| {
        let ownership = ownership.read().expect("ownership lock").clone();
        foreign(&home, &ownership, target)
    })
}

/// Whether `path` is another workspace's: in (or holding) its folder, or a project it owns.
fn foreign(home: &WorkspaceHome, ownership: &ProjectOwnership, path: &Path) -> bool {
    home.foreign_part(path).is_some() || ownership.refuses(path, home.name())
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
