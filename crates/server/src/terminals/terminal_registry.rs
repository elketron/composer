use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use base64::Engine;
use composer_protocol::{DeskBy, DeskTab, ServerMessage};
use portable_pty::{CommandBuilder, native_pty_system};

use crate::server::Server;
use crate::session::Session;
use crate::terminals::Scrollback;
use crate::terminals::terminal::{Terminal, size};

/// How often idle terminals are looked for.
const REAP_EVERY: Duration = Duration::from_secs(60);

/// A workspace's server-owned terminals, by id (ticket 0176).
#[derive(Default)]
pub struct Terminals {
    open: Mutex<HashMap<String, Arc<Terminal>>>,
    reaping: AtomicBool,
}

impl Terminals {
    /// Opens a terminal for `session` (`id`: again, one that ended, where it was), a tab on its
    /// desk: its id and its scrollback (empty for a new one).
    pub fn open(
        &self,
        server: &Arc<Server>,
        session: &Arc<Session>,
        id: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<(String, Vec<u8>), String> {
        if let Some(id) = id
            && let Some(alive) = self.get(id)
        {
            owned(&alive, session)?;
            return Ok((alive.id.clone(), alive.scrollback()));
        }
        let id = match id {
            Some(id) => plain(id)?,
            None => format!("t{:08x}", rand::random::<u32>()),
        };
        let cwd = remembered(session, &id).unwrap_or_else(|| place(server, session));
        let terminal = spawn(server, session, &id, &cwd, cols, rows)?;
        remember(session, &id, &cwd);
        self.open
            .lock()
            .expect("terminals lock")
            .insert(id.clone(), terminal.clone());
        read_into(server, terminal);
        self.reap_later(server);
        let desk = session.change_desk(|desk| {
            desk.open(DeskTab::Terminal { id: id.clone() }, true, DeskBy::User)
                .map(|_| ())
        })?;
        server.hub.send(ServerMessage::DeskChanged {
            session: session.id().to_string(),
            desk,
            by: DeskBy::User,
            opened: None,
        });
        Ok((id, Vec::new()))
    }

    pub fn get(&self, id: &str) -> Option<Arc<Terminal>> {
        self.open.lock().expect("terminals lock").get(id).cloned()
    }

    /// The terminal `id`, or what to do when there is none.
    pub fn find(&self, id: &str) -> Result<Arc<Terminal>, String> {
        self.get(id).ok_or_else(|| {
            format!("no terminal `{id}` running (it ended: `terminal.open` with its id starts a fresh one)")
        })
    }

    /// Ends `id`.
    pub fn close(&self, server: &Server, id: &str) -> Result<(), String> {
        let terminal = self
            .open
            .lock()
            .expect("terminals lock")
            .remove(id)
            .ok_or_else(|| format!("no terminal `{id}` running"))?;
        terminal.kill();
        exited(server, &terminal);
        Ok(())
    }

    /// Ends the terminals nobody used for `idle`; their ids.
    pub fn reap(&self, server: &Server, idle: Duration) -> Vec<String> {
        let stale: Vec<Arc<Terminal>> = {
            let mut open = self.open.lock().expect("terminals lock");
            let ids: Vec<String> = open
                .values()
                .filter(|t| t.idle_for() >= idle)
                .map(|t| t.id.clone())
                .collect();
            ids.iter().filter_map(|id| open.remove(id)).collect()
        };
        for terminal in &stale {
            terminal.kill();
            exited(server, terminal);
        }
        stale.iter().map(|t| t.id.clone()).collect()
    }

    /// Starts the reaper once.
    fn reap_later(&self, server: &Arc<Server>) {
        if self.reaping.swap(true, Ordering::SeqCst) {
            return;
        }
        let server = Arc::downgrade(server);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(REAP_EVERY).await;
                let Some(server) = server.upgrade() else {
                    return;
                };
                let idle = Duration::from_secs(server.config().terminals.idle_hours * 3600);
                server.terminals.reap(&server, idle);
            }
        });
    }

    fn forget(&self, id: &str) -> Option<Arc<Terminal>> {
        self.open.lock().expect("terminals lock").remove(id)
    }
}

/// The terminal belongs to `session`.
pub fn owned(terminal: &Terminal, session: &Session) -> Result<(), String> {
    match terminal.session == session.id() {
        true => Ok(()),
        false => Err(format!("terminal `{}` isn't this session's", terminal.id)),
    }
}

fn plain(id: &str) -> Result<String, String> {
    let ok = !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    ok.then(|| id.to_string())
        .ok_or_else(|| format!("`{id}` isn't a terminal id"))
}

/// Where a new terminal starts: the session's project, else the workspace's sites.
fn place(server: &Server, session: &Session) -> PathBuf {
    session
        .info()
        .project
        .and_then(|p| server.see_project(&p).ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| server.home.sites())
}

/// The shell, in the sandbox (decision 2026-11-07: the user's terminals too), on a new PTY.
fn spawn(
    server: &Arc<Server>,
    session: &Session,
    id: &str,
    cwd: &Path,
    cols: u16,
    rows: u16,
) -> Result<Arc<Terminal>, String> {
    let mut spec = server.sandbox().tty().in_dir(cwd);
    if let Some(project) = session
        .info()
        .project
        .and_then(|p| server.see_project(&p).ok())
    {
        spec = spec.with_rw(project);
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let (program, args) = spec.wrap(&shell)?;
    let pair = native_pty_system()
        .openpty(size(cols, rows))
        .map_err(|e| format!("a terminal: {e}"))?;
    let mut command = builder(&program);
    command.args(args);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.cwd(cwd);
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|e| format!("starting {shell}: {e}"))?;
    drop(pair.slave);
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let cap = server.config().terminals.scrollback_kb * 1024;
    Ok(Arc::new(Terminal {
        id: id.to_string(),
        session: session.id().to_string(),
        cwd: cwd.to_path_buf(),
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
        scrollback: Mutex::new(Scrollback::new(cap)),
        last_used: Mutex::new(Instant::now()),
    }))
}

#[allow(
    clippy::disallowed_methods,
    reason = "the program is the sandbox's bwrap with the shell after it (or the shell, with the \
              sandbox off)"
)]
fn builder(program: &std::ffi::OsStr) -> CommandBuilder {
    CommandBuilder::new(program)
}

/// Reads its output on a thread of its own: into the scrollback and to the session's clients;
/// when the shell ends, it is gone.
fn read_into(server: &Arc<Server>, terminal: Arc<Terminal>) {
    let reader = terminal
        .master
        .lock()
        .expect("terminal pty lock")
        .try_clone_reader();
    let Ok(mut reader) = reader else { return };
    let server: Weak<Server> = Arc::downgrade(server);
    std::thread::spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        while let Ok(n) = reader.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let Some(server) = server.upgrade() else {
                return;
            };
            terminal
                .scrollback
                .lock()
                .expect("scrollback lock")
                .push(&buffer[..n]);
            server.hub.send(ServerMessage::TerminalOutput {
                session: terminal.session.clone(),
                terminal: terminal.id.clone(),
                data: base64::engine::general_purpose::STANDARD.encode(&buffer[..n]),
            });
        }
        if let Some(server) = server.upgrade()
            && server.terminals.forget(&terminal.id).is_some()
        {
            exited(&server, &terminal);
        }
    });
}

fn exited(server: &Server, terminal: &Terminal) {
    server.hub.send(ServerMessage::TerminalExited {
        session: terminal.session.clone(),
        terminal: terminal.id.clone(),
    });
}

/// Where the session's terminal `id` was (`terminals.json` in its folder).
fn remembered(session: &Session, id: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(session.dir().join("terminals.json")).ok()?;
    let places: HashMap<String, PathBuf> = serde_json::from_str(&text).ok()?;
    places.get(id).cloned().filter(|p| p.is_dir())
}

fn remember(session: &Session, id: &str, cwd: &Path) {
    let file = session.dir().join("terminals.json");
    let mut places: HashMap<String, PathBuf> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    places.insert(id.to_string(), cwd.to_path_buf());
    let json = serde_json::to_string_pretty(&places).expect("places serialize");
    if let Err(e) = std::fs::write(&file, json) {
        tracing::warn!("{}: {e}", file.display());
    }
}
