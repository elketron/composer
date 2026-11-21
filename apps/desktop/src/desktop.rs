//! The app: one window on the home in one process (CEF allows one per process), and what it
//! shares: the home and the tokio runtime the client crate runs on.

use std::path::PathBuf;

use composer_client::{Connection, Endpoint};
use gpui::{App, AppContext, Bounds, Global, WindowBounds, WindowHandle, WindowOptions, px, size};

use crate::shell::Shell;

pub struct Desktop {
    pub home: PathBuf,
    runtime: tokio::runtime::Handle,
    window: Option<WindowHandle<Shell>>,
}

impl Global for Desktop {}

impl Desktop {
    pub fn init(home: PathBuf, runtime: tokio::runtime::Handle, cx: &mut App) {
        cx.set_global(Self {
            home,
            runtime,
            window: None,
        });
    }

    /// Runs `work` on the tokio runtime (HTTP needs it); await the handle from anywhere.
    pub fn spawn<T: Send + 'static>(
        &self,
        work: impl Future<Output = T> + Send + 'static,
    ) -> tokio::task::JoinHandle<T> {
        self.runtime.spawn(work)
    }
}

/// Shows the home's window, opening it if there is none.
pub fn open(cx: &mut App) {
    let existing = cx.global::<Desktop>().window;
    if let Some(handle) = existing
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let home = cx.global::<Desktop>().home.clone();
    let endpoint = match Endpoint::discover(&home) {
        Ok(endpoint) => endpoint,
        Err(e) => {
            eprintln!("composer-desktop: {e}");
            return;
        }
    };
    // The client's tasks need the runtime's context.
    let _entered = cx.global::<Desktop>().runtime.enter();
    let (connection, events) = Connection::open(endpoint.clone());
    let bounds = Bounds::centered(None, size(px(1500.), px(950.)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some("composer".into()),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("composer".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| Shell::new(endpoint, connection, events, cx)),
    );
    match opened {
        Ok(handle) => cx.global_mut::<Desktop>().window = Some(handle),
        Err(e) => eprintln!("composer-desktop: a window: {e}"),
    }
}
