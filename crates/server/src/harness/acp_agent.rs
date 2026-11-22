use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::v1::schema::{
    CancelNotification, ContentBlock, EnvVariable, Implementation, InitializeRequest, McpServer,
    McpServerStdio, NewSessionRequest, PermissionOption, PermissionOptionKind, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId as AcpSessionId, SessionNotification, StopReason,
    TextContent,
};
use agent_client_protocol::v1::{ConnectionTo, Responder};
use agent_client_protocol::{Agent, ByteStreams, Client};
use composer_protocol::{ApprovalDecision, ApprovalKind, TurnState};
use serde_json::Value;
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::watch;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tokio_util::sync::CancellationToken;

use crate::actions::Ask;
use crate::harness::{AcpConfig, AcpTurn, ChildProcess, WorkerScope};
use crate::home::Home;
use crate::server::Server;
use crate::session::{RunSink, Session};

/// How long a stopped turn waits for the agent to answer `session/cancel` before it ends anyway.
const CANCEL_GRACE: Duration = Duration::from_secs(10);

/// The agent's side of the conversation, once `initialize` and `session/new` are done.
#[derive(Clone)]
struct Ready {
    connection: ConnectionTo<Agent>,
    session: AcpSessionId,
}

/// An outside agent running one composer session, over the Agent Client Protocol on its
/// stdin/stdout: a process composer started, in its own process group (and systemd scope where
/// there is one), so ending it ends what it started.
///
/// Composer is the ACP client: it starts the agent in the session site's folder, hands it
/// composer itself as its only MCP server (`composer mcp --session <id>`), sends each turn's
/// messages as a `session/prompt`, shows its `session/update`s as the session's events, asks the
/// user its `session/request_permission`s, and stops a turn with `session/cancel`.
pub struct AcpAgent {
    server: Arc<Server>,
    session: Arc<Session>,
    process: ChildProcess,
    ready: watch::Sender<Option<Result<Ready, String>>>,
    /// The turn running now, which the agent's updates go to.
    turn: Mutex<Option<AcpTurn>>,
    /// Ends the connection.
    closing: CancellationToken,
    /// How the process ended, once it has.
    ended: watch::Sender<Option<String>>,
}

impl AcpAgent {
    /// Starts `config`'s agent for `session` and opens its ACP session in the background.
    pub fn launch(
        server: &Arc<Server>,
        session: &Arc<Session>,
        config: &AcpConfig,
    ) -> Result<Arc<Self>, String> {
        let info = session.info();
        let cwd = server
            .session_folder(&info)
            .or_else(|| {
                let site = server.home.sites().join(info.site.as_deref()?);
                site.is_dir().then_some(site)
            })
            .unwrap_or_else(|| server.home.sites());
        let (mut command, scope) =
            WorkerScope::command(OsStr::new(&config.command), session.id(), "");
        let stderr = std::fs::File::create(session.dir().join("agent.log"))
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());
        command
            .args(&config.args)
            .envs(&config.env)
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr);
        let process = ChildProcess::spawn_in(command, scope)
            .map_err(|e| format!("starting `{}`: {e}", config.command))?;
        let (Some(stdin), Some(stdout)) = (process.take_stdin(), process.take_stdout()) else {
            process.interrupt();
            return Err(format!("`{}` has no stdin/stdout", config.command));
        };
        let agent = Arc::new(Self {
            server: server.clone(),
            session: session.clone(),
            process,
            ready: watch::Sender::new(None),
            turn: Mutex::new(None),
            closing: CancellationToken::new(),
            ended: watch::Sender::new(None),
        });
        let request = NewSessionRequest::new(cwd)
            .mcp_servers(vec![composer_as_mcp_server(server, session.id())]);
        tokio::spawn(agent.clone().connect(stdin, stdout, request));
        let watched = agent.clone();
        tokio::spawn(async move {
            let how = watched.process.wait().await;
            tracing::info!("session {}: the agent ended ({how})", watched.session.id());
            watched.closing.cancel();
            watched.ready.send_if_modified(|ready| match ready {
                None => {
                    *ready = Some(Err(format!("the agent ended ({how})")));
                    true
                }
                Some(_) => false,
            });
            watched.ended.send_replace(Some(how));
        });
        Ok(agent)
    }

    /// Runs the ACP connection until the process ends or [`AcpAgent::stop`].
    async fn connect(
        self: Arc<Self>,
        stdin: ChildStdin,
        stdout: ChildStdout,
        new_session: NewSessionRequest,
    ) {
        let transport = ByteStreams::new(stdin.compat_write(), stdout.compat());
        let (on_update, on_permission, main) = (self.clone(), self.clone(), self.clone());
        let connected = Client
            .v1()
            .name("composer")
            .on_receive_notification(
                async move |notification: SessionNotification, _connection| {
                    on_update.update(notification);
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest,
                            responder: Responder<RequestPermissionResponse>,
                            _connection| {
                    on_permission.permission(request, responder);
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(transport, async move |connection: ConnectionTo<Agent>| {
                let initialize = InitializeRequest::new(ProtocolVersion::V1)
                    .client_info(Implementation::new("composer", env!("CARGO_PKG_VERSION")));
                connection.send_request(initialize).block_task().await?;
                let created = connection.send_request(new_session).block_task().await?;
                main.ready.send_replace(Some(Ok(Ready {
                    connection: connection.clone(),
                    session: created.session_id,
                })));
                main.closing.cancelled().await;
                Ok(())
            })
            .await;
        let error = match connected {
            Ok(()) => "the agent's connection closed".to_string(),
            Err(e) => format!("the agent: {e}"),
        };
        self.ready.send_if_modified(|ready| match ready {
            None => {
                *ready = Some(Err(error.clone()));
                true
            }
            Some(_) => false,
        });
        // A connection that broke while the process lives: it is no use any more.
        self.process.interrupt();
    }

    /// Ends the agent and what it started.
    pub fn stop(&self) {
        self.closing.cancel();
        self.process.interrupt();
    }

    /// Resolves once the process has ended.
    pub async fn ended(&self) {
        let mut ended = self.ended.subscribe();
        let _ = ended.wait_for(Option::is_some).await;
    }

    /// One turn: `text` as a `session/prompt`, its updates into `sink` as they come. The result
    /// is the agent's answer (its last text). `cancel` sends `session/cancel`.
    pub async fn prompt(
        &self,
        sink: Arc<dyn RunSink>,
        text: String,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        let mut ready = self.ready.subscribe();
        let ready = tokio::select! {
            ready = ready.wait_for(Option::is_some) => match ready {
                Ok(ready) => ready.clone().expect("waited for it"),
                Err(_) => Err("the agent went away".into()),
            },
            _ = cancel.cancelled() => return Ok(Value::String(String::new())),
        }?;
        sink.set_state(TurnState::Running);
        *self.turn.lock().expect("turn lock") = Some(AcpTurn::new(sink, cancel.clone()));
        let request = PromptRequest::new(
            ready.session.clone(),
            vec![ContentBlock::Text(TextContent::new(text))],
        );
        let sent = ready.connection.send_request(request).block_task();
        tokio::pin!(sent);
        let mut ended = self.ended.subscribe();
        let response = tokio::select! {
            response = &mut sent => Some(response),
            _ = async { let _ = ended.wait_for(Option::is_some).await; } => None,
            _ = cancel.cancelled() => {
                let cancelled = CancelNotification::new(ready.session.clone());
                if let Err(e) = ready.connection.send_notification(cancelled) {
                    tracing::warn!("session {}: session/cancel: {e}", self.session.id());
                }
                // The agent ends the turn with `cancelled`; one that doesn't is not waited for.
                tokio::time::timeout(CANCEL_GRACE, &mut sent).await.ok()
            }
        };
        let turn = self.turn.lock().expect("turn lock").take();
        let answer = turn.map(AcpTurn::finish).unwrap_or_default();
        match response {
            None if cancel.is_cancelled() => Ok(Value::String(answer)),
            None => Err(match self.ended.borrow().clone() {
                Some(how) => format!("the agent ended ({how})"),
                None => "the agent stopped answering".into(),
            }),
            Some(Err(e)) => Err(format!("the agent: {e}")),
            Some(Ok(response)) => match response.stop_reason {
                StopReason::EndTurn | StopReason::Cancelled => Ok(Value::String(answer)),
                StopReason::MaxTokens => Err("the agent ran out of tokens".into()),
                StopReason::MaxTurnRequests => Err("the agent reached its turn limit".into()),
                StopReason::Refusal => Err("the agent refused".into()),
                other => Err(format!("the agent stopped: {other:?}")),
            },
        }
    }

    /// A `session/update`: to the running turn, if there is one (between turns, the agent's
    /// commands, modes and the like are nothing composer shows).
    fn update(&self, notification: SessionNotification) {
        if let Some(turn) = self.turn.lock().expect("turn lock").as_mut() {
            turn.update(notification.update);
        }
    }

    /// A `session/request_permission`: asked in composer's clients as a tool approval while the
    /// turn runs; the answer goes back as the agent's matching option. A stopped turn answers
    /// `cancelled`, as the protocol asks.
    fn permission(
        &self,
        request: RequestPermissionRequest,
        responder: Responder<RequestPermissionResponse>,
    ) {
        let cancel = self
            .turn
            .lock()
            .expect("turn lock")
            .as_ref()
            .map(|turn| turn.cancel.clone());
        let Some(cancel) = cancel else {
            let _ = responder.respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
            return;
        };
        let (server, session) = (self.server.clone(), self.session.clone());
        tokio::spawn(async move {
            let fields = &request.tool_call.fields;
            let title = fields
                .title
                .clone()
                .unwrap_or_else(|| "The agent asks to use a tool".into());
            let mut body = match &fields.raw_input {
                Some(input) => serde_json::to_string_pretty(input).unwrap_or_default(),
                None => String::new(),
            };
            if !body.is_empty() {
                body.push_str("\n\n");
            }
            let names: Vec<&str> = request.options.iter().map(|o| o.name.as_str()).collect();
            body.push_str(&format!("The agent's options: {}", names.join(" / ")));
            let always = option(&request.options, &[PermissionOptionKind::AllowAlways])
                .map(|o| o.name.clone());
            let what = format!("`{title}`");
            let ask = Ask::action(ApprovalKind::Tool, what, title, body, always);
            let decision = server
                .ask(&session, ask, &cancel)
                .await
                .map(|answer| answer.decision);
            let outcome = outcome(&request.options, decision);
            if let Err(e) = responder.respond(RequestPermissionResponse::new(outcome)) {
                tracing::warn!("session {}: answering the agent: {e}", session.id());
            }
        });
    }
}

/// The agent's option for the user's answer: allow once (or always, if that is all it offers),
/// allow always (or once), reject (once or always); no answer, or no option that fits, is
/// `cancelled`.
fn outcome(
    options: &[PermissionOption],
    decision: Option<ApprovalDecision>,
) -> RequestPermissionOutcome {
    use PermissionOptionKind::*;
    let kinds: &[PermissionOptionKind] = match decision {
        Some(ApprovalDecision::Approve) => &[AllowOnce, AllowAlways],
        Some(ApprovalDecision::Always) => &[AllowAlways, AllowOnce],
        Some(ApprovalDecision::Deny) => &[RejectOnce, RejectAlways],
        None => &[],
    };
    match option(options, kinds) {
        Some(chosen) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
            chosen.option_id.clone(),
        )),
        None => RequestPermissionOutcome::Cancelled,
    }
}

/// The first option of the first of `kinds` the agent offers.
fn option<'a>(
    options: &'a [PermissionOption],
    kinds: &[PermissionOptionKind],
) -> Option<&'a PermissionOption> {
    kinds
        .iter()
        .find_map(|kind| options.iter().find(|o| o.kind == *kind))
}

/// Composer itself, as the agent's MCP server: this binary's `composer mcp` for the session,
/// connecting back to this server in this home.
fn composer_as_mcp_server(server: &Server, session: &str) -> McpServer {
    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("composer"));
    let address = server
        .listen_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| server.config().server.bind.clone());
    McpServer::Stdio(
        McpServerStdio::new("composer", program)
            .args(vec![
                "mcp".into(),
                "--session".into(),
                session.into(),
                "--server".into(),
                format!("http://{address}"),
            ])
            .env(vec![EnvVariable::new(
                Home::ENV_HOME,
                server.home.root().display().to_string(),
            )]),
    )
}
