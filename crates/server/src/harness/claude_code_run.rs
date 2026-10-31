use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use composer_protocol::{ChatMessage, Role, ServerMessage, TurnState};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStdout;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::harness::stream_json::{StreamEvent, parse_line};
use crate::harness::{ChildProcess, HarnessRun, Pause, StartRequest, WorkerSink};
use crate::server::Server;
use crate::worker::ticket::set_ticket_fields;

/// Tool output beyond this is cut in the worker's chat (the log keeps all of it).
const RESULT_MAX_CHARS: usize = 4000;

/// A running `claude -p`: its stream-json goes to the log and, mapped, to the worker session.
pub struct ClaudeCodeRun {
    process: ChildProcess,
    reader: Mutex<Option<JoinHandle<()>>>,
    seen: Arc<Mutex<Seen>>,
    /// Cancelled once the process has ended (stops the timeout).
    ended: CancellationToken,
    server: Arc<Server>,
    ticket: String,
}

/// What the stream said that matters after the run.
#[derive(Default)]
struct Seen {
    pause: Option<Pause>,
    resets_at: Option<i64>,
    result: Option<String>,
    timed_out: bool,
    /// The result was `error_max_turns`.
    max_turns: bool,
}

impl ClaudeCodeRun {
    pub fn start(
        server: &Arc<Server>,
        request: StartRequest,
        process: ChildProcess,
        log: File,
    ) -> Result<Arc<Self>, String> {
        let stdout = process
            .take_stdout()
            .ok_or("Claude Code's output is not piped")?;
        let sink = WorkerSink::new(server, &request.session).ok_or("no worker session")?;
        let run = Arc::new(Self {
            process,
            reader: Mutex::new(None),
            seen: Arc::new(Mutex::new(Seen::default())),
            ended: CancellationToken::new(),
            server: server.clone(),
            ticket: request.ticket.clone(),
        });
        let reader = tokio::spawn(read(
            stdout,
            log,
            sink,
            run.seen.clone(),
            server.clone(),
            request.ticket,
        ));
        *run.reader.lock().expect("reader lock") = Some(reader);

        let (timer, minutes) = (run.clone(), server.config().claude_code.timeout_minutes);
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(minutes * 60)) => {
                    timer.seen.lock().expect("seen lock").timed_out = true;
                    timer.process.interrupt();
                }
                _ = timer.ended.cancelled() => {}
            }
        });
        Ok(run)
    }
}

#[async_trait]
impl HarnessRun for ClaudeCodeRun {
    async fn wait(&self) -> String {
        let exit = self.process.wait().await;
        self.ended.cancel();
        let reader = self.reader.lock().expect("reader lock").take();
        if let Some(reader) = reader {
            let _ = reader.await;
        }
        if let Err(e) = set_ticket_fields(&self.server, &self.ticket, &[("exit", &exit)]) {
            tracing::warn!("recording the exit of ticket {}: {e}", self.ticket);
        }
        let seen = self.seen.lock().expect("seen lock");
        let minutes = self.server.config().claude_code.timeout_minutes;
        match (&seen.result, seen.timed_out) {
            (_, true) => format!("stopped after {minutes} minutes"),
            (Some(said), false) if !said.trim().is_empty() => {
                format!("{exit}; it last said: {}", cut(said.trim(), 300))
            }
            _ => exit,
        }
    }

    fn interrupt(&self) {
        self.process.interrupt();
    }

    fn paused(&self) -> Option<Pause> {
        self.seen.lock().expect("seen lock").pause.clone()
    }

    fn stuck(&self) -> Option<String> {
        let seen = self.seen.lock().expect("seen lock");
        (seen.max_turns && !seen.timed_out).then(|| {
            let max_turns = self.server.config().claude_code.max_turns;
            match seen
                .result
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(said) => format!(
                    "used its {max_turns} turns; it last said: {}",
                    cut(said, 600)
                ),
                None => format!("used its {max_turns} turns"),
            }
        })
    }
}

/// Reads the stream until the process closes it.
async fn read(
    stdout: ChildStdout,
    mut log: File,
    sink: WorkerSink,
    seen: Arc<Mutex<Seen>>,
    server: Arc<Server>,
    ticket: String,
) {
    let session = sink.session().to_string();
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let _ = writeln!(log, "{line}");
        for event in parse_line(&line) {
            apply(event, &sink, &seen, &server, &ticket, &session);
        }
    }
}

fn apply(
    event: StreamEvent,
    sink: &WorkerSink,
    seen: &Mutex<Seen>,
    server: &Server,
    ticket: &str,
    session: &str,
) {
    let session = session.to_string();
    match event {
        StreamEvent::Init { model, .. } => {
            if let Err(e) = set_ticket_fields(server, ticket, &[("model", &model)]) {
                tracing::warn!("recording the model of ticket {ticket}: {e}");
            }
            sink.set_state(TurnState::Running);
            sink.event(ServerMessage::TurnStarted { session, seq: 0 });
        }
        StreamEvent::Text(text) => sink.event(ServerMessage::MessageDone {
            session,
            seq: 0,
            message: assistant(text, None),
        }),
        StreamEvent::Thinking(thinking) => sink.event(ServerMessage::MessageDone {
            session,
            seq: 0,
            message: assistant(String::new(), Some(thinking)),
        }),
        StreamEvent::TextDelta(text) => sink.event(ServerMessage::MessageDelta {
            session,
            seq: 0,
            text,
        }),
        StreamEvent::ThinkingDelta(text) => sink.event(ServerMessage::ThinkingDelta {
            session,
            seq: 0,
            text,
        }),
        StreamEvent::ToolUse {
            id,
            name,
            input,
            parent,
        } => sink.event(ServerMessage::ToolCallStarted {
            session,
            seq: 0,
            call_id: id,
            // composer's own tools under their own names.
            name: name
                .strip_prefix("mcp__composer__")
                .unwrap_or(&name)
                .to_string(),
            args: input,
            parent,
        }),
        StreamEvent::ToolResult {
            id,
            output,
            is_error,
        } => sink.event(ServerMessage::ToolResult {
            session,
            seq: 0,
            call_id: id,
            result: cut(&output, RESULT_MAX_CHARS),
            is_error,
        }),
        StreamEvent::RateLimit {
            rejected,
            resets_at,
        } => {
            let mut seen = seen.lock().expect("seen lock");
            seen.resets_at = resets_at.or(seen.resets_at);
            if rejected {
                seen.pause = Some(Pause {
                    reason: "Claude Code hit its usage limit".into(),
                    until: seen.resets_at,
                });
            }
        }
        StreamEvent::Result {
            subtype,
            is_error,
            text,
            api_error_status,
            turns,
            duration_ms,
            cost_usd,
        } => {
            let stats = [
                turns.map(|t| ("turns", t.to_string())),
                duration_ms.map(|ms| ("duration", duration(ms))),
                cost_usd.map(|c| ("cost_usd", format!("{c:.4}"))),
            ];
            let stats: Vec<(&str, &str)> = stats
                .iter()
                .flatten()
                .map(|(k, v)| (*k, v.as_str()))
                .collect();
            if !stats.is_empty()
                && let Err(e) = set_ticket_fields(server, ticket, &stats)
            {
                tracing::warn!("recording the stats of ticket {ticket}: {e}");
            }
            {
                let mut seen = seen.lock().expect("seen lock");
                let limited = api_error_status == Some(429)
                    || text.to_lowercase().contains("usage limit")
                    || text.to_lowercase().contains("limit reached");
                if is_error && limited && seen.pause.is_none() {
                    seen.pause = Some(Pause {
                        reason: "Claude Code hit its usage limit".into(),
                        until: seen.resets_at,
                    });
                }
                seen.max_turns = subtype == "error_max_turns";
                seen.result = Some(text);
            }
            sink.event(ServerMessage::TurnDone {
                session,
                seq: 0,
                cancelled: false,
            });
            sink.set_state(TurnState::Idle);
        }
    }
}

fn assistant(content: String, thinking: Option<String>) -> ChatMessage {
    ChatMessage {
        role: Role::Assistant,
        content,
        thinking,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        attachments: vec![],
        source: None,
    }
}

/// `14s`, `3m 20s`, `1h 05m`.
fn duration(ms: i64) -> String {
    let secs = ms / 1000;
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {:02}s", s / 60, s % 60),
        s => format!("{}h {:02}m", s / 3600, s % 3600 / 60),
    }
}

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}
