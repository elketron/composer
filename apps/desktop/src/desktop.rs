//! The app: one window per workspace in one process (CEF allows one per process), and what the
//! windows share: the home, the tokio runtime the client crate runs on, workers per workspace.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use composer_client::{Connection, Endpoint};
use gpui::{App, AppContext, Bounds, Global, WindowBounds, WindowHandle, WindowOptions, px, size};

use crate::shell::Shell;
use crate::workspaces;

pub struct Desktop {
    pub home: PathBuf,
    runtime: tokio::runtime::Handle,
    windows: HashMap<String, WindowHandle<Shell>>,
    /// Running workers per workspace (`/api/projects`, every 10 s).
    workers: HashMap<String, u64>,
}

impl Global for Desktop {}

const WORKERS_EVERY: Duration = Duration::from_secs(10);

impl Desktop {
    pub fn init(home: PathBuf, runtime: tokio::runtime::Handle, cx: &mut App) {
        cx.set_global(Self {
            home,
            runtime,
            windows: HashMap::new(),
            workers: HashMap::new(),
        });
        cx.spawn(async move |cx| {
            loop {
                let (home, runtime) =
                    cx.read_global(|d: &Desktop, _| (d.home.clone(), d.runtime.clone()));
                let counted = runtime
                    .spawn(async move {
                        let mut counts = HashMap::new();
                        for name in workspaces::list(&home) {
                            let Ok(endpoint) = Endpoint::discover(&home, &name) else {
                                continue;
                            };
                            let projects: Vec<serde_json::Value> =
                                endpoint.get("/api/projects").await.unwrap_or_default();
                            let running =
                                projects.iter().filter_map(|p| p["running"].as_u64()).sum();
                            counts.insert(name, running);
                        }
                        counts
                    })
                    .await;
                if let Ok(counts) = counted {
                    cx.update_global(|d: &mut Desktop, cx| {
                        d.workers = counts;
                        cx.refresh_windows();
                    });
                }
                cx.background_executor().timer(WORKERS_EVERY).await;
            }
        })
        .detach();
    }

    /// Runs `work` on the tokio runtime (HTTP needs it); await the handle from anywhere.
    pub fn spawn<T: Send + 'static>(
        &self,
        work: impl Future<Output = T> + Send + 'static,
    ) -> tokio::task::JoinHandle<T> {
        self.runtime.spawn(work)
    }

    pub fn workers(&self) -> u64 {
        self.workers.values().sum()
    }

    pub fn workspaces(&self) -> Vec<String> {
        workspaces::list(&self.home)
    }
}

/// Shows workspace `name`'s window, opening it if there is none.
pub fn open(name: &str, cx: &mut App) {
    let existing = cx.global::<Desktop>().windows.get(name).copied();
    if let Some(handle) = existing
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let home = cx.global::<Desktop>().home.clone();
    let endpoint = match Endpoint::discover(&home, name) {
        Ok(endpoint) => endpoint,
        Err(e) => {
            eprintln!("composer-desktop: {e}");
            return;
        }
    };
    let colour = workspaces::colour(&home, name);
    // The client's tasks need the runtime's context.
    let _entered = cx.global::<Desktop>().runtime.enter();
    let (connection, events) = Connection::open(endpoint.clone());
    let bounds = Bounds::centered(None, size(px(1500.), px(950.)), cx);
    let workspace = name.to_string();
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("composer".into()),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some(format!("composer — {name}").into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| Shell::new(workspace, colour, endpoint, connection, events, cx)),
    );
    match opened {
        Ok(handle) => {
            cx.global_mut::<Desktop>()
                .windows
                .insert(name.to_string(), handle);
        }
        Err(e) => eprintln!("composer-desktop: a window for {name}: {e}"),
    }
}
