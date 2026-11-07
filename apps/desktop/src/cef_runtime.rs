//! CEF for the whole process: its helper processes, settings and cache. One per process (its
//! cache dir is a single-instance lock), so every window shares it.

use std::path::PathBuf;

use cef::{args::Args, *};

/// Where the runtime is: `$CEF_PATH`; beside the binary (a package); else the one a development
/// build used (`.cef`, see fetch-cef.sh).
fn cef_dir() -> String {
    if let Ok(dir) = std::env::var("CEF_PATH") {
        return dir;
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .filter(|dir| dir.join("libcef.so").is_file());
    match beside {
        Some(dir) => dir.display().to_string(),
        None => env!("COMPOSER_CEF_DIR").to_string(),
    }
}

fn cache_root() -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        });
    cache.join("composer/cef")
}

/// Chromium switches for every CEF process. The browser shows the workspace's apps with their
/// tokens, so nothing else runs in it: no extensions (Chromium's runtime otherwise loads the
/// system's, like a desktop's browser integration), no sync, background networking or first-run
/// UI.
const SWITCHES: [&str; 7] = [
    "disable-extensions",
    "disable-component-extensions-with-background-pages",
    "disable-background-networking",
    "disable-sync",
    "no-first-run",
    "no-default-browser-check",
    "disable-default-apps",
];

wrap_app! {
    pub struct ComposerApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            if let Some(command_line) = command_line {
                for switch in SWITCHES {
                    command_line.append_switch(Some(&switch.into()));
                }
                // Software compositing: offscreen frames then don't wait on a GPU process,
                // which here started late or not at all (first paint delayed or missing, ticket
                // 0183). WebGL still works (SwiftShader). `COMPOSER_CEF_GPU=1` turns the GPU on.
                if std::env::var_os("COMPOSER_CEF_GPU").is_none() {
                    for switch in ["disable-gpu", "disable-gpu-compositing"] {
                        command_line.append_switch(Some(&switch.into()));
                    }
                }
                // Chromium's own log on stderr, on request (`COMPOSER_CEF_LOG=1`).
                if std::env::var_os("COMPOSER_CEF_LOG").is_some() {
                    command_line.append_switch_with_value(Some(&"enable-logging".into()), Some(&"stderr".into()));
                    command_line.append_switch_with_value(Some(&"v".into()), Some(&"0".into()));
                }
            }
        }
    }
}

/// CEF re-runs this binary for its renderer, GPU and utility processes: those end here. Must be
/// the first thing `main` does.
pub fn run_helper_process() {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let mut app = ComposerApp::new();
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        std::process::exit(code);
    }
}

/// Starts CEF for this process; its cache folder is removed by [`shutdown`].
pub fn start() -> Result<PathBuf, String> {
    let dir = cef_dir();
    let root = cache_root();
    forget_dead_caches(&root);
    let cache = root.join(std::process::id().to_string());
    // CEF falls back to a shared default profile (and opens a browser window of its own) when it
    // can't use this folder; it doesn't create the parents.
    std::fs::create_dir_all(&cache).map_err(|e| format!("CEF cache {}: {e}", cache.display()))?;
    let settings = Settings {
        windowless_rendering_enabled: 1,
        external_message_pump: 1,
        no_sandbox: 1,
        root_cache_path: cache.to_string_lossy().as_ref().into(),
        resources_dir_path: dir.as_str().into(),
        locales_dir_path: format!("{dir}/locales").as_str().into(),
        // DevTools for the panes, on request only (`COMPOSER_CEF_DEBUG_PORT=9229`).
        remote_debugging_port: std::env::var("COMPOSER_CEF_DEBUG_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0),
        ..Default::default()
    };
    let args = Args::new();
    let mut app = ComposerApp::new();
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        return Err(format!(
            "CEF didn't start: see its messages above (its runtime: {dir}; scripts/fetch-cef.sh puts one there)"
        ));
    }
    Ok(cache)
}

pub fn shutdown(cache: PathBuf) {
    cef::shutdown();
    let _ = std::fs::remove_dir_all(cache);
}

/// Caches of instances that are gone (crashed, killed).
fn forget_dead_caches(root: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let alive = entry
            .file_name()
            .to_str()
            .and_then(|pid| pid.parse::<u32>().ok())
            .is_some_and(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists());
        if !alive {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
