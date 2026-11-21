//! The composer desktop app (plan phase 7): one window on the home, with the chat on the left
//! and the stage on the right (the session's desk). It links only `composer-client`, never the
//! server.
//!
//!   composer-desktop     (home: $COMPOSER_HOME, else ~/.composer)

mod browser_frame;
mod browser_pump;
mod browser_tabs;
mod cef_runtime;
mod chat_model;
mod chat_pane;
mod chat_width;
mod desktop;
mod dom_keys;
mod draft;
mod grid;
mod home_lists;
mod home_view;
mod key_map;
mod link_state;
mod markdown;
mod matcher;
mod notice;
mod outgoing;
mod overlay_frame;
mod page_refresh;
mod pick_lists;
mod picker;
mod prose;
mod refresher;
mod shell;
mod stage_tabs;
mod term_config;
mod term_pane;
mod terminal_link;
mod theme;
mod uploads;
mod view_urls;
mod web_pane;

use std::borrow::Cow;
use std::path::PathBuf;

use composer_client::Endpoint;
use gpui::{App, KeyBinding};

use desktop::Desktop;
use shell::{FocusChat, FocusStage, Home, NextTab, Palette, PickSession, PrevTab};

const FONTS: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf"),
    // The chat's prose. Strong and italic need their own faces: gpui would otherwise match a
    // system font, and the transcript would go mixed-typeface. Same OFL family (Inter-LICENSE).
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Italic.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBoldItalic.ttf"),
];

fn home() -> PathBuf {
    match std::env::var_os("COMPOSER_HOME") {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .unwrap_or_default(),
        )
        .join(".composer"),
    }
}

/// Start-up timing on stderr, on request (`COMPOSER_DESKTOP_TRACE=1`).
pub fn trace(step: &str) {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    if std::env::var_os("COMPOSER_DESKTOP_TRACE").is_some() {
        eprintln!("[{:>6} ms] {step}", start.elapsed().as_millis());
    }
}

/// Starts the desktop. Windows supplies the sandbox information from CEF's bootstrap executable.
pub fn run(sandbox_info: *mut u8) -> i32 {
    trace("start");
    // CEF re-runs this binary for its own processes: before anything else.
    if let Some(code) = cef_runtime::run_helper_process(sandbox_info) {
        return code;
    }
    trace("not a CEF helper");
    let home = home();
    // Its server found before anything opens: a clear message instead of an empty window.
    if let Err(e) = Endpoint::discover(&home) {
        eprintln!("composer-desktop: {e}");
        return 1;
    }
    // CEF before any other threads: its start-up owns the process's descriptors.
    let cache = match cef_runtime::start(sandbox_info) {
        Ok(cache) => cache,
        Err(e) => {
            eprintln!("composer-desktop: {e}");
            return 1;
        }
    };
    trace("CEF started");
    // The client's connection runs on tokio; gpui runs the windows on the main thread.
    let runtime = tokio::runtime::Runtime::new().expect("a tokio runtime");
    let _entered = runtime.enter();
    let handle = runtime.handle().clone();

    let application = gpui_platform::application();
    trace("application");
    application.run(move |cx: &mut App| {
        trace("gpui running");
        cx.text_system()
            .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
            .expect("the bundled font loads");
        trace("fonts");
        // The shell's own keys, few: everything else goes to the focused pane.
        cx.bind_keys([
            KeyBinding::new("ctrl-1", FocusChat, None),
            KeyBinding::new("ctrl-2", FocusStage, None),
            KeyBinding::new("ctrl-tab", NextTab, None),
            KeyBinding::new("ctrl-shift-tab", PrevTab, None),
            KeyBinding::new("ctrl-shift-p", Palette, None),
            KeyBinding::new("ctrl-shift-s", PickSession, None),
            KeyBinding::new("ctrl-shift-h", Home, None),
        ]);
        browser_pump::start(cx);
        trace("cef pump");
        Desktop::init(home, handle, cx);
        trace("desktop");
        desktop::open(cx);
        trace("window opened");
        cx.activate(true);
    });
    cef_runtime::shutdown(cache);
    0
}

/// The sandboxed Windows bootstrap loads this DLL for the browser and each helper process.
#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "C" fn RunWinMain(
    _instance: cef::sys::HINSTANCE,
    _command_line: *const u16,
    _command_show: i32,
    sandbox_info: *mut u8,
    _version_info: *mut u8,
) -> i32 {
    if sandbox_info.is_null() {
        return 1;
    }
    run(sandbox_info)
}
