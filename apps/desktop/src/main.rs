//! The composer desktop app (plan phase 7): a window per workspace with the chat on the left (your
//! TUI, run by the server for this window) and the stage on the right (the session's desk). It
//! links only `composer-client`, never the server.
//!
//!   composer-desktop [--workspace <name>]     (home: $COMPOSER_HOME, else ~/.composer)

mod browser_pump;
mod browser_tabs;
mod cef_runtime;
mod chat_width;
mod dom_keys;
mod grid;
mod key_map;
mod link_state;
mod shell;
mod stage_tabs;
mod term_pane;
mod terminal_link;
mod view_urls;
mod web_pane;

use std::borrow::Cow;
use std::path::PathBuf;

use composer_client::{Connection, Endpoint};
use gpui::{App, Bounds, KeyBinding, WindowBounds, WindowOptions, prelude::*, px, size};

use shell::{FocusChat, FocusStage, NextTab, PrevTab, Shell};

const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf"),
];

fn home() -> PathBuf {
    match std::env::var_os("COMPOSER_HOME") {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".composer"),
    }
}

fn workspace() -> String {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == "--workspace" || a == "-w")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| "default".into())
}

fn main() {
    // CEF re-runs this binary for its own processes: before anything else.
    cef_runtime::run_helper_process();
    let workspace = workspace();
    let endpoint = match Endpoint::discover(&home(), &workspace) {
        Ok(endpoint) => endpoint,
        Err(e) => {
            eprintln!("composer-desktop: {e}");
            std::process::exit(1);
        }
    };
    // CEF before any other threads: its start-up owns the process's descriptors.
    let cache = match cef_runtime::start() {
        Ok(cache) => cache,
        Err(e) => {
            eprintln!("composer-desktop: {e}");
            std::process::exit(1);
        }
    };
    // The client's connection runs on tokio; gpui runs the window on the main thread.
    let runtime = tokio::runtime::Runtime::new().expect("a tokio runtime");
    let _entered = runtime.enter();

    gpui_platform::application().run(move |cx: &mut App| {
        cx.text_system()
            .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
            .expect("the bundled font loads");
        cx.bind_keys([
            KeyBinding::new("ctrl-1", FocusChat, None),
            KeyBinding::new("ctrl-2", FocusStage, None),
            KeyBinding::new("ctrl-tab", NextTab, None),
            KeyBinding::new("ctrl-shift-tab", PrevTab, None),
        ]);
        browser_pump::start(cx);
        let (connection, events) = Connection::open(endpoint.clone());
        let bounds = Bounds::centered(None, size(px(1500.), px(950.)), cx);
        let title = format!("composer — {workspace}");
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("composer".into()),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| cx.new(|cx| Shell::new(workspace, endpoint, connection, events, cx)),
        )
        .expect("a window");
        cx.activate(true);
    });
    cef_runtime::shutdown(cache);
}
