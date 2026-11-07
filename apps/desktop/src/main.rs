//! The composer desktop app (plan phase 7): a window per workspace with the chat on the left (your
//! TUI, run by the server for this window) and the stage on the right (the session's desk). It
//! links only `composer-client`, never the server.
//!
//!   composer-desktop [--workspace <name>]     (home: $COMPOSER_HOME, else ~/.composer)

mod browser_pump;
mod browser_tabs;
mod cef_runtime;
mod chat_width;
mod desktop;
mod dom_keys;
mod grid;
mod key_map;
mod link_state;
mod matcher;
mod pick_lists;
mod picker;
mod shell;
mod stage_tabs;
mod term_pane;
mod terminal_link;
mod view_urls;
mod web_pane;
mod workspaces;

use std::borrow::Cow;
use std::path::PathBuf;

use composer_client::Endpoint;
use gpui::{App, KeyBinding};

use desktop::Desktop;
use shell::{
    FocusChat, FocusStage, NextTab, Palette, PickSession, PickWorkspace, PrevTab, Workspace1,
    Workspace2, Workspace3, Workspace4, Workspace5, Workspace6, Workspace7, Workspace8, Workspace9,
};

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
    let (home, workspace) = (home(), workspace());
    // Its server found before anything opens: a clear message instead of an empty window.
    if let Err(e) = Endpoint::discover(&home, &workspace) {
        eprintln!("composer-desktop: {e}");
        std::process::exit(1);
    }
    // CEF before any other threads: its start-up owns the process's descriptors.
    let cache = match cef_runtime::start() {
        Ok(cache) => cache,
        Err(e) => {
            eprintln!("composer-desktop: {e}");
            std::process::exit(1);
        }
    };
    // The client's connection runs on tokio; gpui runs the windows on the main thread.
    let runtime = tokio::runtime::Runtime::new().expect("a tokio runtime");
    let _entered = runtime.enter();
    let handle = runtime.handle().clone();

    gpui_platform::application().run(move |cx: &mut App| {
        cx.text_system()
            .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
            .expect("the bundled font loads");
        // The shell's own keys, few: everything else goes to the focused pane.
        cx.bind_keys([
            KeyBinding::new("ctrl-1", FocusChat, None),
            KeyBinding::new("ctrl-2", FocusStage, None),
            KeyBinding::new("ctrl-tab", NextTab, None),
            KeyBinding::new("ctrl-shift-tab", PrevTab, None),
            KeyBinding::new("ctrl-shift-p", Palette, None),
            KeyBinding::new("ctrl-shift-w", PickWorkspace, None),
            KeyBinding::new("ctrl-shift-s", PickSession, None),
            KeyBinding::new("ctrl-alt-1", Workspace1, None),
            KeyBinding::new("ctrl-alt-2", Workspace2, None),
            KeyBinding::new("ctrl-alt-3", Workspace3, None),
            KeyBinding::new("ctrl-alt-4", Workspace4, None),
            KeyBinding::new("ctrl-alt-5", Workspace5, None),
            KeyBinding::new("ctrl-alt-6", Workspace6, None),
            KeyBinding::new("ctrl-alt-7", Workspace7, None),
            KeyBinding::new("ctrl-alt-8", Workspace8, None),
            KeyBinding::new("ctrl-alt-9", Workspace9, None),
        ]);
        browser_pump::start(cx);
        Desktop::init(home, handle, cx);
        desktop::open(&workspace, cx);
        cx.activate(true);
    });
    cef_runtime::shutdown(cache);
}
