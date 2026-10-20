use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use composer_core::Page;
use composer_protocol::{
    ChangedBy, ChatMessage, MessageSource, Role, ServerMessage, TurnState, WorkerState,
};

use crate::agent::AgentDefinition;
use crate::harness::{
    ClaudeCodeHarness, Harness, OwnHarness, Resume, StartRequest, check_harness_allowed,
    project_settings,
};
use crate::project::{Worktree, ticket_worktree};
use crate::server::Server;
use crate::session::{Session, submit};
use crate::tools::{ToolContext, resolve_project};
use crate::worker::WorkerHandle;
use crate::worker::ticket::{set_field, set_status, ticket_url};

/// How long a worker that called `finish` has to exit before it is stopped.
const FINISHED_GRACE: Duration = Duration::from_secs(60);

/// Workers started by this server, by worker session id.
#[derive(Default)]
pub struct WorkerRegistry {
    running: Mutex<HashMap<String, WorkerHandle>>,
    /// Harnesses that hit a usage limit, until when (unix seconds).
    paused: Mutex<HashMap<&'static str, i64>>,
}

/// What a launch needs besides the harness's own request.
struct Launch {
    harness: Arc<dyn Harness>,
    session: Arc<Session>,
    parent: String,
    page: Page,
    request: StartRequest,
}

impl WorkerRegistry {
    pub fn running(&self) -> Vec<(String, WorkerHandle)> {
        self.running
            .lock()
            .expect("workers lock")
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// The harnesses a ticket's `harness:` can name; the first is the default.
    fn harnesses() -> Vec<Arc<dyn Harness>> {
        vec![Arc::new(OwnHarness), Arc::new(ClaudeCodeHarness)]
    }

    fn harness(name: &str) -> Result<Arc<dyn Harness>, String> {
        let harnesses = Self::harnesses();
        harnesses
            .iter()
            .find(|h| h.name() == name)
            .cloned()
            .ok_or_else(|| {
                let names: Vec<&str> = harnesses.iter().map(|h| h.name()).collect();
                format!("no harness `{name}`; available: {}", names.join(", "))
            })
    }

    pub fn get(&self, worker: &str) -> Option<WorkerHandle> {
        self.running
            .lock()
            .expect("workers lock")
            .get(worker)
            .cloned()
    }

    /// What a harness's tool calls (`tool.call`) run against: the running worker's session
    /// and agent, on this server.
    pub fn tool_context(&self, server: &Arc<Server>, worker: &str) -> Result<ToolContext, String> {
        if self.get(worker).is_none() {
            return Err(format!("#{worker} is not a running worker"));
        }
        let session = server.sessions.get(worker).map_err(|e| e.to_string())?;
        let agent = AgentDefinition::load(&server.workspace, &session.info().agent)
            .map_err(|e| e.to_string())?;
        Ok(ToolContext {
            server: server.clone(),
            session,
            agent: Arc::new(agent),
            cancel: CancellationToken::new(),
        })
    }

    /// Stops a running worker; its ticket fails as cancelled.
    pub fn interrupt(&self, worker: &str) -> bool {
        let Some(handle) = self.get(worker) else {
            return false;
        };
        handle.cancelled.store(true, Ordering::SeqCst);
        handle.run.interrupt();
        true
    }

    /// Starts a worker on ticket `id` for the orchestrator `parent`, running `agent` (else the
    /// ticket's `agent:`, else `worker`) on `harness` (else the ticket's `harness:`, else
    /// composer's own). Returns the worker's session id and, for a git project, the worktree
    /// it works in.
    pub async fn spawn(
        &self,
        server: &Arc<Server>,
        parent: &Arc<Session>,
        id: &str,
        agent: Option<&str>,
        harness: Option<&str>,
    ) -> Result<(String, Option<Worktree>), String> {
        self.check_capacity(server, id)?;
        let page = ticket_page(server, id)?;
        match page.frontmatter.get_str("status").as_deref() {
            Some("claimed") => return Err(format!("ticket {id} is already claimed")),
            Some("done") => {
                return Err(format!(
                    "ticket {id} is done; open a new ticket for more work"
                ));
            }
            _ => {}
        }
        let agent = agent
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .or_else(|| page.frontmatter.get_str("agent").filter(|a| !a.is_empty()))
            .unwrap_or_else(|| "worker".into());
        check_worker_agent(server, &agent)?;
        let harness = Self::harness(
            &harness
                .map(|h| h.trim().to_string())
                .filter(|h| !h.is_empty())
                .or_else(|| {
                    page.frontmatter
                        .get_str("harness")
                        .filter(|h| !h.is_empty())
                })
                .unwrap_or_else(|| Self::harnesses()[0].name().to_string()),
        )?;
        let (project, worktree, settings) =
            self.place(server, &harness, &page, id, parent.info().project)?;

        let session = server
            .sessions
            .create(&agent, project.clone())
            .map_err(|e| e.to_string())?;
        let sid = session.id().to_string();
        session.update_info(&server.hub, |info| {
            info.ticket = Some(id.to_string());
            info.parent = Some(parent.id().to_string());
            info.preview = Some(format!("ticket {id}: {}", page.title()));
        });
        let mut claimed = set_status(&page.raw, "claimed");
        if let Some(w) = &worktree {
            claimed = set_field(&claimed, "branch", &w.branch);
        }
        if harness.external() {
            claimed = set_field(&claimed, "harness", harness.name());
        }
        server
            .write_page(&page.url, &claimed, ChangedBy::Agent)
            .map_err(|e| e.to_string())?;

        let request = StartRequest {
            session: sid.clone(),
            ticket: id.to_string(),
            agent,
            project,
            settings,
            log: log_path(server, &sid)?,
            resume: None,
        };
        self.launch(
            server,
            Launch {
                harness,
                session,
                parent: parent.id().to_string(),
                page,
                request,
            },
        )
        .await?;
        Ok((sid, worktree))
    }

    /// Whether a message to `session` continues its ticket's run instead of starting a turn
    /// here: a stopped worker of a harness that can resume (Claude Code, by its session id).
    pub fn resumable(&self, server: &Server, session: &Session) -> bool {
        let info = session.info();
        let Some(ticket) = info.ticket else {
            return false;
        };
        self.get(&info.id).is_none()
            && ticket_page(server, &ticket).is_ok_and(|page| {
                page.frontmatter
                    .get_str("harness_session")
                    .is_some_and(|s| !s.is_empty())
            })
    }

    /// Continues the stopped worker `session` on its ticket with the user's `message`.
    pub async fn resume(
        &self,
        server: &Arc<Server>,
        session: &Arc<Session>,
        message: String,
    ) -> Result<(), String> {
        let info = session.info();
        let id = info.ticket.clone().ok_or("not a worker session")?;
        let parent = info
            .parent
            .clone()
            .ok_or("the worker has no orchestrator")?;
        self.check_capacity(server, &id)?;
        let page = ticket_page(server, &id)?;
        if page.frontmatter.get_str("status").as_deref() == Some("claimed") {
            return Err(format!("ticket {id} is claimed by another worker"));
        }
        let harness_session = page
            .frontmatter
            .get_str("harness_session")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("ticket {id} has no session to resume"))?;
        let harness = Self::harness(
            &page
                .frontmatter
                .get_str("harness")
                .unwrap_or_else(|| "composer".into()),
        )?;
        let parent_project = server
            .sessions
            .get(&parent)
            .ok()
            .and_then(|p| p.info().project);
        let (project, _, settings) = self.place(server, &harness, &page, &id, parent_project)?;
        server
            .write_page(
                &page.url,
                &set_status(&page.raw, "claimed"),
                ChangedBy::Agent,
            )
            .map_err(|e| e.to_string())?;
        // What you wrote shows in the worker's chat, as in any session.
        session.emit(&server.hub, |seq| ServerMessage::MessageDone {
            session: info.id.clone(),
            seq,
            message: user_message(message.clone(), MessageSource::Tui),
        });
        let request = StartRequest {
            session: info.id.clone(),
            ticket: id,
            agent: info.agent.clone(),
            project,
            settings,
            log: log_path(server, &info.id)?,
            resume: Some(Resume {
                harness_session,
                message,
            }),
        };
        self.launch(
            server,
            Launch {
                harness,
                session: session.clone(),
                parent,
                page,
                request,
            },
        )
        .await
    }

    fn check_capacity(&self, server: &Server, id: &str) -> Result<(), String> {
        let running = self.running();
        if running.len() >= server.config.agent.max_workers as usize {
            let busy: Vec<String> = running
                .iter()
                .map(|(s, h)| format!("#{s} on {}", h.ticket))
                .collect();
            return Err(format!(
                "{} worker(s) already running ({}); wait for the finish message",
                running.len(),
                busy.join(", ")
            ));
        }
        if running.iter().any(|(_, h)| h.ticket == id) {
            return Err(format!("ticket {id} already has a worker"));
        }
        Ok(())
    }

    /// Where the ticket is worked: the folder (its worktree in a git project), the worktree,
    /// and the project's settings. Refuses an external harness the project doesn't allow, or
    /// one that is paused.
    fn place(
        &self,
        server: &Server,
        harness: &Arc<dyn Harness>,
        page: &Page,
        id: &str,
        parent_project: Option<String>,
    ) -> Result<(Option<String>, Option<Worktree>, crate::ProjectSettings), String> {
        if server.listen_addr().is_none() {
            return Err("the server is not listening yet".into());
        }
        if let Some(until) = self.paused_until(harness.name()) {
            return Err(format!(
                "`{}` is paused until {} (usage limit)",
                harness.name(),
                local_time(until)
            ));
        }
        let project = page
            .frontmatter
            .get_str("project")
            .filter(|p| !p.is_empty())
            .and_then(|p| resolve_project(&p, &server.config.projects.dir).ok())
            .or(parent_project);
        // Checked against the project itself, before it becomes a worktree path.
        let settings = if harness.external() {
            check_harness_allowed(&server.config, project.as_deref(), harness.name())?
        } else {
            project
                .as_deref()
                .map(|p| project_settings(&server.config, p))
                .unwrap_or_default()
        };
        // Its own worktree (git projects), so workers on one project don't collide.
        let worktree = match &project {
            Some(folder) => ticket_worktree(
                Path::new(folder),
                &server.home.state().join("worktrees"),
                id,
            )?,
            None => None,
        };
        let project = worktree
            .as_ref()
            .map(|w| w.path.to_string_lossy().to_string())
            .or(project);
        Ok((project, worktree, settings))
    }

    /// The pause of `harness`, if it hasn't run out yet.
    fn paused_until(&self, harness: &str) -> Option<i64> {
        let mut paused = self.paused.lock().expect("paused lock");
        let until = *paused.get(harness)?;
        if until > chrono::Utc::now().timestamp() {
            Some(until)
        } else {
            paused.remove(harness);
            None
        }
    }

    /// Starts the run and watches it: a run that ends without `finish` fails the ticket (or
    /// pauses it, after a usage limit).
    async fn launch(&self, server: &Arc<Server>, launch: Launch) -> Result<(), String> {
        let Launch {
            harness,
            session,
            parent,
            page,
            request,
        } = launch;
        let (sid, ticket) = (request.session.clone(), request.ticket.clone());
        session.update_info(&server.hub, |info| {
            info.worker = Some(WorkerState::Starting)
        });
        let run = match harness.start(server, request).await {
            Ok(run) => run,
            Err(e) => {
                // Nothing runs: give the ticket back and show the session as failed.
                let _ = server.write_page(&page.url, &page.raw, ChangedBy::Agent);
                session.update_info(&server.hub, |info| info.worker = Some(WorkerState::Failed));
                return Err(e);
            }
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        self.running.lock().expect("workers lock").insert(
            sid.clone(),
            WorkerHandle {
                ticket: ticket.clone(),
                parent: parent.clone(),
                harness: harness.name(),
                run: run.clone(),
                cancelled: cancelled.clone(),
            },
        );
        server.hub.send(ServerMessage::WorkerStatus {
            worker: sid.clone(),
            ticket,
            parent,
            state: WorkerState::Starting,
        });

        let (watch_server, name) = (server.clone(), harness.name());
        tokio::spawn(async move {
            let ended = run.wait().await;
            if watch_server.workers.get(&sid).is_none() {
                return;
            }
            let (state, summary) = if cancelled.load(Ordering::SeqCst) {
                (WorkerState::Failed, "cancelled by the user".to_string())
            } else if let Some(pause) = run.paused() {
                if let Some(until) = pause.until {
                    watch_server
                        .workers
                        .paused
                        .lock()
                        .expect("paused lock")
                        .insert(name, until);
                }
                let when = pause
                    .until
                    .map(|u| format!("; it can go on after {}", local_time(u)))
                    .unwrap_or_default();
                (
                    WorkerState::Paused,
                    format!("{}{when}. Send it a message to resume.", pause.reason),
                )
            } else {
                (
                    WorkerState::Failed,
                    format!(
                        "the worker ended ({ended}) without calling finish; its log is state/workers/{sid}.log"
                    ),
                )
            };
            watch_server
                .workers
                .finish(&watch_server, &sid, state, summary);
        });
        Ok(())
    }

    /// A worker is done (or died, or paused): update the ticket, tell clients and the
    /// orchestrator.
    pub fn finish(&self, server: &Arc<Server>, worker: &str, state: WorkerState, summary: String) {
        let Some(handle) = self.running.lock().expect("workers lock").remove(worker) else {
            return;
        };
        // It should exit on its own now; one that lingers is stopped.
        let run = handle.run.clone();
        tokio::spawn(async move {
            tokio::time::sleep(FINISHED_GRACE).await;
            run.interrupt();
        });
        let status = match state {
            WorkerState::Done => "done",
            WorkerState::Paused => "paused",
            _ => "failed",
        };
        let mut branch = None;
        if let Ok(page) = ticket_page(server, &handle.ticket) {
            branch = page.frontmatter.get_str("branch").filter(|b| !b.is_empty());
            if let Err(e) =
                server.write_page(&page.url, &set_status(&page.raw, status), ChangedBy::Agent)
            {
                tracing::error!("updating ticket {}: {e}", handle.ticket);
            }
        }
        if let Ok(session) = server.sessions.get(worker) {
            session.update_info(&server.hub, |info| {
                info.state = TurnState::Idle;
                info.worker = Some(state);
            });
        }
        server.hub.send(ServerMessage::WorkerStatus {
            worker: worker.to_string(),
            ticket: handle.ticket.clone(),
            parent: handle.parent.clone(),
            state,
        });
        if let Ok(orchestrator) = server.sessions.get(&handle.parent) {
            let on_branch = branch
                .map(|b| format!(" Its changes are on branch `{b}`."))
                .unwrap_or_default();
            let content = format!(
                "[worker #{worker} finished ticket {ticket}: {status}]\n{summary}\n\nIts Result is in site://orchestration/tickets/{ticket}.md.{on_branch}",
                ticket = handle.ticket
            );
            let message = user_message(
                content,
                MessageSource::Worker {
                    session: worker.to_string(),
                },
            );
            submit(server, &orchestrator, message);
        }
    }
}

fn ticket_page(server: &Server, id: &str) -> Result<Page, String> {
    let url = ticket_url(id).map_err(|e| e.to_string())?;
    server.workspace.read_page(&url).map_err(|_| {
        format!("no ticket {id}; create it with `run orchestration new-ticket <title>`")
    })
}

fn check_worker_agent(server: &Server, agent: &str) -> Result<(), String> {
    let definition = AgentDefinition::load(&server.workspace, agent).map_err(|e| e.to_string())?;
    if definition.is_worker() {
        return Ok(());
    }
    let index = server.index.read().expect("index lock");
    let workers: Vec<String> = AgentDefinition::workers(&server.workspace, &index)
        .into_iter()
        .map(|w| w.name)
        .collect();
    Err(format!(
        "`{agent}` is not a worker agent (its page needs `role: worker`); worker agents: {}",
        if workers.is_empty() {
            "none".to_string()
        } else {
            workers.join(", ")
        }
    ))
}

fn log_path(server: &Server, session: &str) -> Result<std::path::PathBuf, String> {
    let logs = server.home.state().join("workers");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    Ok(logs.join(format!("{session}.log")))
}

fn user_message(content: String, source: MessageSource) -> ChatMessage {
    ChatMessage {
        role: Role::User,
        content,
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        source: Some(source),
    }
}

/// `14:05` today, else `Oct 21 14:05` (local time).
fn local_time(unix: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_opt(unix, 0).single() else {
        return unix.to_string();
    };
    if time.date_naive() == Local::now().date_naive() {
        time.format("%H:%M").to_string()
    } else {
        time.format("%b %-d %H:%M").to_string()
    }
}
