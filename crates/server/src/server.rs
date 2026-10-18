use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use composer_core::{Index, Page, SiteUrl, Workspace};
use composer_protocol::{ChangedBy, PageChangeKind, ServerMessage};
use tokio::net::TcpListener;

use crate::config::Config;
use crate::error::{Result, ServerError};
use crate::hash::content_hash;
use crate::home::Home;
use crate::hub::EventHub;
use crate::init::local_token;
use crate::provider::{OpenAiProvider, Provider};
use crate::session::SessionManager;
use crate::slot_pool::SlotPool;
use crate::tools::ToolRegistry;

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
    /// Required on every request (header or `?token=`).
    pub token: String,
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
        Ok(Arc::new(Self {
            token: local_token(&home)?,
            sessions: SessionManager::load(&home.sessions())?,
            slots: SlotPool::new(config.agent.slots, hub.clone()),
            tools: ToolRegistry::standard(),
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

    /// Watches the workspace and serves the API until the listener fails.
    pub async fn serve(self: &Arc<Self>, listener: TcpListener) -> Result<()> {
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
        let created = self.workspace.locate(url).is_err();
        let written = self.workspace.write_page(url, content)?;
        let path = self.workspace.locate(&written)?;
        self.known_hashes
            .lock()
            .expect("hashes lock")
            .insert(path, content_hash(content.as_bytes()));
        self.index_page(&written, content);
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
