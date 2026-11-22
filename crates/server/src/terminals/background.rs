//! A background process ending: its process group goes with it, its clients are told, and so
//! is the agent, with a short message into its session (on exit only, never on output).

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use composer_protocol::{ChatMessage, MessageSource, Role};
use portable_pty::ExitStatus;

use crate::server::Server;
use crate::session::{note, submit};
use crate::terminals::Terminal;
use crate::terminals::terminal_registry::{GRACE, exited as terminal_exited};
use crate::terminals::types::StoppedBy;

/// Lines of its output the exit message carries.
pub const EXIT_LINES: usize = 20;

/// How long the rest of its output gets to arrive after its process exited.
const DRAIN: Duration = Duration::from_secs(2);

/// The process of background terminal `terminal` exited (`status`: how, when known). Called on
/// the thread that waited for it.
pub(crate) fn exited(
    server: &Weak<Server>,
    terminal: &Arc<Terminal>,
    status: Option<ExitStatus>,
    runtime: Option<tokio::runtime::Handle>,
) {
    // What it left running in its group goes with it.
    #[cfg(unix)]
    if let Some(pid) = terminal.pid() {
        crate::harness::signal_group(pid, libc::SIGHUP);
        crate::harness::signal_group(pid, libc::SIGTERM);
        std::thread::spawn(move || {
            std::thread::sleep(GRACE);
            crate::harness::signal_group(pid, libc::SIGKILL);
        });
    }
    let deadline = Instant::now() + DRAIN;
    while !terminal.is_drained() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let Some(server) = server.upgrade() else {
        return;
    };
    if server.terminals.forget(&terminal.id).is_none() {
        return;
    }
    terminal_exited(&server, terminal);
    server
        .terminals
        .processes_changed(&server, &terminal.session);
    let by = terminal.stopped_by();
    if matches!(by, Some(StoppedBy::Agent | StoppedBy::Server)) {
        return;
    }
    let Ok(session) = server.sessions.get(&terminal.session) else {
        return;
    };
    let Some(runtime) = runtime else {
        tracing::warn!(
            "background process {}: no runtime to tell its session",
            terminal.id
        );
        return;
    };
    let command = terminal
        .background
        .as_ref()
        .map(|b| b.command.clone())
        .unwrap_or_default();
    let message = ChatMessage {
        role: Role::User,
        content: message_text(terminal, &command, status.as_ref(), by),
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        attachments: vec![],
        source: Some(MessageSource::Process {
            terminal: terminal.id.clone(),
            command,
        }),
    };
    let _entered = runtime.enter();
    // The user stopped it: they know. The agent reads the note with its next turn instead of
    // being woken for it.
    if matches!(by, Some(StoppedBy::User)) {
        note(&server, &session, message);
    } else {
        submit(&server, &session, message);
    }
}

/// "Background process t… (`cmd`) exited with code 1. Its last lines: …"
fn message_text(
    terminal: &Terminal,
    command: &str,
    status: Option<&ExitStatus>,
    by: Option<StoppedBy>,
) -> String {
    let how = match (by, status) {
        (Some(_), _) => "was stopped by the user".to_string(),
        (None, Some(status)) => match status.signal() {
            Some(signal) => format!("was killed ({signal})"),
            None => format!("exited with code {}", status.exit_code()),
        },
        (None, None) => "ended".to_string(),
    };
    let lines = terminal.text(EXIT_LINES);
    let tail = if lines.is_empty() {
        "No output.".to_string()
    } else {
        format!("Its last lines:\n```\n{lines}\n```")
    };
    format!(
        "[automatic notice: background process {} (`{command}`) {how}]\n{tail}",
        terminal.id
    )
}
