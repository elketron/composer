use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use base64::Engine;
use composer_protocol::{DeskBy, DeskTab, ProcessInfo, ServerMessage};
use portable_pty::{Child, CommandBuilder, native_pty_system};

use crate::server::Server;
use crate::session::Session;
use crate::terminals::Scrollback;
use crate::terminals::terminal::{Terminal, size};
use crate::terminals::types::{Background, StoppedBy};

/// How long a stopped process group gets to exit before it is killed.
pub(crate) const GRACE: Duration = Duration::from_secs(3);

/// A workspace's server-owned terminals, by id (ticket 0176).
#[derive(Default)]
pub struct Terminals {
    open: Mutex<HashMap<String, Arc<Terminal>>>,
}

impl Terminals {
    /// Opens a terminal for `session` (`id`: again, one that ended, where it was), a tab on its
    /// desk: its id and its scrollback (empty for a new one). With `command`, that program runs
    /// in it (through the user's shell) instead of a shell.
    pub fn open(
        &self,
        server: &Arc<Server>,
        session: &Arc<Session>,
        id: Option<&str>,
        command: Option<&str>,
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
            None => new_id(),
        };
        let cwd = remembered(session, &id).unwrap_or_else(|| place(server, session));
        let command = shell(command.filter(|c| !c.trim().is_empty()));
        let (terminal, child) = spawn(
            server,
            session.id(),
            &id,
            command,
            &cwd,
            size(cols, rows),
            None,
        )?;
        remember(session, &id, &cwd);
        self.insert(server, terminal, child);
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

    /// Starts a `type: tui` page's program in a terminal on `session`'s desk. It is a client
    /// (like the desktop TUI); the page has already checked the program against its `control:`
    /// adapter.
    pub fn open_tui(
        &self,
        server: &Arc<Server>,
        session: &Arc<Session>,
        command: &[String],
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<String, String> {
        let Some(program) = command.first() else {
            return Err("a TUI page has an empty command".into());
        };
        let id = new_id();
        let mut builder = CommandBuilder::new(program);
        builder.args(command[1..].iter());
        let (terminal, child) = spawn(
            server,
            session.id(),
            &id,
            builder,
            cwd,
            size(cols, rows),
            None,
        )?;
        self.insert(server, terminal, child);
        self.show_for_agent(server, session, &id, true)?;
        Ok(id)
    }

    /// Starts `command` as a background process of `session` (`process_start`): in a terminal
    /// of its own, through the user's shell, in `cwd`; a tab on its desk the agent opened. It
    /// runs until the agent or the user stops it, or the server stops.
    pub fn start_process(
        &self,
        server: &Arc<Server>,
        session: &Arc<Session>,
        command: &str,
        cwd: &Path,
    ) -> Result<String, String> {
        let id = new_id();
        let background = Background {
            command: command.to_string(),
            started: chrono::Utc::now().to_rfc3339(),
        };
        let (terminal, child) = spawn(
            server,
            session.id(),
            &id,
            shell(Some(command)),
            cwd,
            size(120, 32),
            Some(background),
        )?;
        self.insert(server, terminal, child);
        self.processes_changed(server, session.id());
        self.show_for_agent(server, session, &id, false)?;
        Ok(id)
    }

    /// `session`'s background processes that are running, oldest first.
    pub fn processes(&self, session: &str) -> Vec<ProcessInfo> {
        let mut processes: Vec<ProcessInfo> = self
            .open
            .lock()
            .expect("terminals lock")
            .values()
            .filter(|t| t.session == session)
            .filter_map(|t| {
                let background = t.background.as_ref()?;
                Some(ProcessInfo {
                    terminal: t.id.clone(),
                    command: background.command.clone(),
                    cwd: t.cwd.display().to_string(),
                    started: background.started.clone(),
                })
            })
            .collect();
        processes.sort_by(|a, b| a.started.cmp(&b.started));
        processes
    }

    /// Tells `session`'s clients its background processes now.
    pub(crate) fn processes_changed(&self, server: &Server, session: &str) {
        server.hub.send(ServerMessage::ProcessesChanged {
            session: session.to_string(),
            processes: self.processes(session),
        });
    }

    /// Stops the background process `id` and its process group. It ends as any exit does: the
    /// agent is told, unless it stopped it itself.
    pub fn stop_process(&self, id: &str, by: StoppedBy) -> Result<(), String> {
        let terminal = self.find(id)?;
        if terminal.background.is_none() {
            return Err(format!(
                "terminal `{id}` isn't a background process (`terminal.close` ends it)"
            ));
        }
        terminal.stop(by, GRACE);
        Ok(())
    }

    /// Ends every terminal (the server is stopping): their process groups are asked to stop,
    /// waited for a little, then killed. Nothing a session started outlives the server.
    pub fn shutdown(&self) {
        let all: Vec<Arc<Terminal>> = self
            .open
            .lock()
            .expect("terminals lock")
            .values()
            .cloned()
            .collect();
        for terminal in &all {
            terminal.stop(StoppedBy::Server, GRACE);
        }
        #[cfg(unix)]
        {
            let deadline = std::time::Instant::now() + GRACE;
            while all.iter().any(|t| t.group_alive()) && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            for terminal in all.iter().filter(|t| t.group_alive()) {
                if let Some(pid) = terminal.pid() {
                    crate::harness::signal_group(pid, libc::SIGKILL);
                }
            }
        }
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

    /// Ends `id`. A background process is stopped by the user: the agent is told.
    pub fn close(&self, server: &Server, id: &str) -> Result<(), String> {
        if self.get(id).is_some_and(|t| t.background.is_some()) {
            return self.stop_process(id, StoppedBy::User);
        }
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

    pub(crate) fn forget(&self, id: &str) -> Option<Arc<Terminal>> {
        self.open.lock().expect("terminals lock").remove(id)
    }

    fn insert(
        &self,
        server: &Arc<Server>,
        terminal: Arc<Terminal>,
        child: Box<dyn Child + Send + Sync>,
    ) {
        self.open
            .lock()
            .expect("terminals lock")
            .insert(terminal.id.clone(), terminal.clone());
        read_into(server, terminal, child);
    }

    /// The terminal `id` as a tab the agent opened on `session`'s desk (`focus`: shown).
    fn show_for_agent(
        &self,
        server: &Server,
        session: &Session,
        id: &str,
        focus: bool,
    ) -> Result<(), String> {
        let desk = session.change_desk(|desk| {
            desk.open(
                DeskTab::Terminal { id: id.to_string() },
                focus,
                DeskBy::Agent,
            )
            .map(|_| ())
        })?;
        server.hub.send(ServerMessage::DeskChanged {
            session: session.id().to_string(),
            desk,
            by: DeskBy::Agent,
            opened: Some(format!("terminal:{id}")),
        });
        Ok(())
    }
}

fn new_id() -> String {
    format!("t{:08x}", rand::random::<u32>())
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

/// Where a new terminal starts: the folder of the session's site, else the sites.
fn place(server: &Server, session: &Session) -> PathBuf {
    server
        .session_folder(&session.info())
        .unwrap_or_else(|| server.home.sites())
}

/// `command` on a new PTY, in `cwd`: a terminal of `session`, and its process to wait for.
fn spawn(
    server: &Server,
    session: &str,
    id: &str,
    mut command: CommandBuilder,
    cwd: &Path,
    size: portable_pty::PtySize,
    background: Option<Background>,
) -> Result<(Arc<Terminal>, Box<dyn Child + Send + Sync>), String> {
    let program = command
        .get_argv()
        .first()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|e| format!("a terminal: {e}"))?;
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.cwd(cwd);
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|e| format!("starting {program}: {e}"))?;
    drop(pair.slave);
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let cap = server.config().terminals.scrollback_kb * 1024;
    let terminal = Arc::new(Terminal {
        id: id.to_string(),
        session: session.to_string(),
        cwd: cwd.to_path_buf(),
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        killer: Mutex::new(child.clone_killer()),
        pid: child.process_id(),
        scrollback: Mutex::new(Scrollback::new(cap)),
        background,
        stopped: Mutex::new(None),
        drained: AtomicBool::new(false),
    });
    Ok((terminal, child))
}

/// The user's shell; with `command`, that run through it.
fn shell(command: Option<&str>) -> CommandBuilder {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut builder = CommandBuilder::new(&shell);
    if let Some(command) = command {
        builder.args(["-c", command]);
    }
    builder
}

/// Reads its output on a thread of its own, into the scrollback and to the session's clients,
/// and waits for its process on another. A shell is gone when its output ends; a background
/// process when its process exits (then its group goes too, and the agent is told).
fn read_into(
    server: &Arc<Server>,
    terminal: Arc<Terminal>,
    mut child: Box<dyn Child + Send + Sync>,
) {
    let weak: Weak<Server> = Arc::downgrade(server);
    let waited = terminal.clone();
    let waiter = weak.clone();
    let runtime = tokio::runtime::Handle::try_current().ok();
    std::thread::spawn(move || {
        let status = child.wait().ok();
        if waited.background.is_some() {
            crate::terminals::background::exited(&waiter, &waited, status, runtime);
        }
    });
    let reader = terminal
        .master
        .lock()
        .expect("terminal pty lock")
        .try_clone_reader();
    let Ok(mut reader) = reader else { return };
    std::thread::spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        while let Ok(n) = reader.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let Some(server) = weak.upgrade() else {
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
        terminal.set_drained();
        if terminal.background.is_some() {
            return;
        }
        if let Some(server) = weak.upgrade()
            && server.terminals.forget(&terminal.id).is_some()
        {
            exited(&server, &terminal);
        }
    });
}

pub(crate) fn exited(server: &Server, terminal: &Terminal) {
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
