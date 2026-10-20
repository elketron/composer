use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use composer_core::Page;
use composer_protocol::{
    Attachment, ChangedBy, ChatMessage, HarnessSwitch, MessageSource, Role, ServerMessage,
    TurnState, WorkerState,
};

use crate::agent::AgentDefinition;
use crate::attachments::{resolve, ticket_files};
use crate::config_sections::ProjectSettings;
use crate::harness::{
    ClaudeCodeHarness, Harness, OwnHarness, Resume, StartRequest, check_harness_allowed,
    pick_model, project_settings,
};
use crate::project::{Worktree, ticket_worktree};
use crate::server::Server;
use crate::session::{Session, submit};
use crate::tools::{ToolContext, resolve_project};
use crate::worker::ending::Ending;
use crate::worker::ticket::{set_field, set_status, ticket_url};
use crate::worker::{StopReason, WorkerHandle, ticket_closed};

/// How long a worker that called `finish` has to exit before it is stopped.
const FINISHED_GRACE: Duration = Duration::from_secs(60);

/// How long shutdown waits for workers to stop.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(10);

/// What a paused worker is told when it goes on by itself.
const GO_ON: &str = "You were paused; you can go on now. Continue the ticket where you left off.";

/// Workers started by this server, by worker session id.
#[derive(Default)]
pub struct WorkerRegistry {
    running: Mutex<HashMap<String, WorkerHandle>>,
    /// Harnesses that hit a usage limit, until when (unix seconds).
    paused: Mutex<HashMap<&'static str, i64>>,
}

/// Where and how a ticket is worked.
struct Place {
    /// The folder the run works in: the worktree, or the project folder.
    folder: Option<String>,
    worktree: Option<Worktree>,
    /// The ticket's project itself.
    project: Option<String>,
    settings: ProjectSettings,
    model: Option<String>,
}

/// What a launch needs besides the harness's own request.
struct Launch {
    harness: Arc<dyn Harness>,
    session: Arc<Session>,
    parent: String,
    page: Page,
    project: Option<String>,
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
        match self.get(worker) {
            Some(handle) => {
                stop(&handle, StopReason::Cancelled);
                true
            }
            None => false,
        }
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
        let place = self.place(server, &harness, &page, id, parent.info().project)?;

        let session = server
            .sessions
            .create(&agent, place.folder.clone())
            .map_err(|e| e.to_string())?;
        let sid = session.id().to_string();
        session.update_info(&server.hub, |info| {
            info.ticket = Some(id.to_string());
            info.parent = Some(parent.id().to_string());
            info.preview = Some(format!("ticket {id}: {}", page.title()));
        });
        let mut claimed = set_status(&page.raw, "claimed");
        if let Some(w) = &place.worktree {
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
            project: place.folder,
            settings: place.settings,
            model: place.model,
            files: ticket_files(
                &server.home,
                page.frontmatter.get("attachments"),
                place.project.as_deref(),
            ),
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
                project: place.project,
                request,
            },
        )
        .await?;
        Ok((sid, place.worktree))
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

    /// Continues the stopped worker `session` on its ticket with the user's `message` (and the
    /// files attached to it, already checked against the session).
    pub async fn resume(
        &self,
        server: &Arc<Server>,
        session: &Arc<Session>,
        message: String,
        attachments: Vec<Attachment>,
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
        if let Some(closed) = ticket_closed(server, &id) {
            return Err(format!("{closed}; open a new ticket for more work"));
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
        let place = self.place(server, &harness, &page, &id, parent_project)?;
        server
            .write_page(
                &page.url,
                &set_status(&page.raw, "claimed"),
                ChangedBy::Agent,
            )
            .map_err(|e| e.to_string())?;
        // What it is told shows in the worker's chat, as in any session.
        let files = attachments
            .iter()
            .filter_map(|a| resolve(&server.home, &info.id, a).ok())
            .map(|(_, path)| path)
            .collect();
        session.emit(&server.hub, |seq| ServerMessage::MessageDone {
            session: info.id.clone(),
            seq,
            message: ChatMessage {
                attachments,
                ..user_message(message.clone(), MessageSource::Tui)
            },
        });
        let request = StartRequest {
            session: info.id.clone(),
            ticket: id,
            agent: info.agent.clone(),
            project: place.folder,
            settings: place.settings,
            model: place.model,
            files: ticket_files(
                &server.home,
                page.frontmatter.get("attachments"),
                place.project.as_deref(),
            ),
            log: log_path(server, &info.id)?,
            resume: Some(Resume {
                harness_session,
                message,
                files,
            }),
        };
        self.launch(
            server,
            Launch {
                harness,
                session: session.clone(),
                parent,
                page,
                project: place.project,
                request,
            },
        )
        .await
    }

    /// The external harnesses for `project` (a folder), with whether config.toml allows them
    /// and whether they are on; plus any harness model turned off there.
    pub fn switches(&self, server: &Server, project: &str) -> Vec<HarnessSwitch> {
        let off = server.switches.off(project);
        let mut switches: Vec<HarnessSwitch> = Self::harnesses()
            .iter()
            .filter(|h| h.external())
            .map(|h| HarnessSwitch {
                entry: h.name().to_string(),
                allowed: check_harness_allowed(&server.config, Some(project), h.name()).is_ok(),
                on: !off.iter().any(|e| e == h.name()),
            })
            .collect();
        for entry in off.iter().filter(|e| e.contains(':')) {
            switches.push(HarnessSwitch {
                entry: entry.clone(),
                allowed: true,
                on: false,
            });
        }
        switches
    }

    /// Turns `entry` (`claude-code`, `claude-code:opus`, `composer`) on or off for `project`.
    /// Off stops its running tickets there (they pause, and go on when it is turned back on).
    pub async fn switch(
        &self,
        server: &Arc<Server>,
        project: &str,
        entry: &str,
        on: bool,
    ) -> Result<(), String> {
        let harness = entry.split(':').next().unwrap_or_default();
        Self::harness(harness)?;
        server.switches.set(project, entry, on)?;
        if !on {
            for (_, handle) in self.running() {
                let model = handle.model.as_deref();
                let matches = handle.harness == harness
                    && (entry == harness
                        || model.is_some_and(|m| entry == format!("{harness}:{m}")));
                if matches && handle.project.as_deref() == Some(project) {
                    stop(&handle, StopReason::SwitchedOff);
                }
            }
            return Ok(());
        }
        // Back on: what it stopped goes on.
        for (session, page) in paused_workers(server, "switch") {
            let ticket_project = page_project(server, &page, &session);
            let ticket_harness = page.frontmatter.get_str("harness").unwrap_or_default();
            if ticket_harness == harness && ticket_project.as_deref() == Some(project) {
                let resumed = self.resume(server, &session, GO_ON.into(), vec![]).await;
                if let Err(e) = resumed {
                    tracing::warn!("resuming #{}: {e}", session.id());
                }
            }
        }
        Ok(())
    }

    /// Stops every running worker (the server is shutting down). External ones pause, to be
    /// resumed with a message; the orchestrators aren't told.
    pub async fn shutdown(&self) {
        let running = self.running();
        if running.is_empty() {
            return;
        }
        tracing::info!("stopping {} worker(s)", running.len());
        for (_, handle) in &running {
            stop(handle, StopReason::Shutdown);
        }
        let deadline = tokio::time::Instant::now() + SHUTDOWN_WAIT;
        while !self.running().is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// At startup: workers paused by a usage limit go on once it has reset.
    pub fn schedule_paused(&self, server: &Arc<Server>) {
        for (session, page) in paused_workers(server, "limit") {
            if let Some(until) = page
                .frontmatter
                .get_str("paused_until")
                .and_then(|u| u.parse().ok())
            {
                go_on_at(server, session.id().to_string(), until);
            }
        }
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

    /// Where and how the ticket is worked. Refuses an external harness the project doesn't
    /// allow, a model it doesn't allow, one turned off for it, or one that is paused.
    fn place(
        &self,
        server: &Server,
        harness: &Arc<dyn Harness>,
        page: &Page,
        id: &str,
        parent_project: Option<String>,
    ) -> Result<Place, String> {
        if server.listen_addr().is_none() {
            return Err("the server is not listening yet".into());
        }
        let name = harness.name();
        if let Some(until) = self.paused_until(name) {
            return Err(format!(
                "`{name}` is paused until {} (usage limit)",
                local_time(until)
            ));
        }
        let project = ticket_project(server, page, parent_project);
        // Checked against the project itself, before it becomes a worktree path.
        let settings = if harness.external() {
            let mut settings = check_harness_allowed(&server.config, project.as_deref(), name)?;
            // What the user said `always` to, in this project.
            if let Some(project) = &project {
                settings
                    .allowed_tools
                    .extend(server.tool_rules.rules(project));
            }
            settings
        } else {
            project
                .as_deref()
                .map(|p| project_settings(&server.config, p))
                .unwrap_or_default()
        };
        let default = harness.default_model(server);
        let model = pick_model(&settings, name, default.as_deref())?;
        if !harness.chooses_model() && model != default {
            return Err(format!(
                "this project allows only {}; `{name}` runs {}",
                settings.models.join(", "),
                default.unwrap_or_default()
            ));
        }
        if let Some(project) = &project
            && server.switches.is_off(project, name, model.as_deref())
        {
            let what = match &model {
                Some(m)
                    if server
                        .switches
                        .off(project)
                        .contains(&format!("{name}:{m}")) =>
                {
                    format!("{name}:{m}")
                }
                _ => name.to_string(),
            };
            return Err(format!(
                "`{what}` is turned off for this project (turn it on with `:harness on {what}`)"
            ));
        }
        // Its own worktree (git projects), so workers on one project don't collide.
        let worktree = match &project {
            Some(folder) => ticket_worktree(
                Path::new(folder),
                &server.home.state().join("worktrees"),
                id,
            )?,
            None => None,
        };
        let folder = worktree
            .as_ref()
            .map(|w| w.path.to_string_lossy().to_string())
            .or(project.clone());
        Ok(Place {
            folder,
            worktree,
            project,
            settings,
            model,
        })
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
    /// pauses it).
    async fn launch(&self, server: &Arc<Server>, launch: Launch) -> Result<(), String> {
        let Launch {
            harness,
            session,
            parent,
            page,
            project,
            request,
        } = launch;
        let (sid, ticket) = (request.session.clone(), request.ticket.clone());
        let model = request.model.clone();
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
        let handle = WorkerHandle {
            ticket: ticket.clone(),
            parent: parent.clone(),
            harness: harness.name(),
            run: run.clone(),
            resumable: harness.external(),
            project,
            model,
            stop: Arc::new(Mutex::new(None)),
        };
        self.running
            .lock()
            .expect("workers lock")
            .insert(sid.clone(), handle.clone());
        server.hub.send(ServerMessage::WorkerStatus {
            worker: sid.clone(),
            ticket,
            parent,
            state: WorkerState::Starting,
        });

        let watch_server = server.clone();
        tokio::spawn(async move {
            let ended = run.wait().await;
            if watch_server.workers.get(&sid).is_none() {
                return;
            }
            let name = handle.harness;
            let reason = *handle.stop.lock().expect("stop lock");
            let ending = match reason {
                Some(StopReason::Cancelled) => Ending::failed("cancelled by the user"),
                Some(StopReason::SwitchedOff) if handle.resumable => Ending::paused(
                    format!(
                        "`{name}` was turned off for this project; it goes on when turned back on"
                    ),
                    "switch",
                    None,
                ),
                Some(StopReason::SwitchedOff) => {
                    Ending::failed(format!("`{name}` was turned off for this project"))
                }
                Some(StopReason::Shutdown) if handle.resumable => Ending {
                    notify: false,
                    ..Ending::paused(
                        "the server stopped; send it a message to go on".into(),
                        "shutdown",
                        None,
                    )
                },
                Some(StopReason::Shutdown) => Ending {
                    notify: false,
                    ..Ending::failed("the server stopped")
                },
                None => match run.paused() {
                    Some(pause) => {
                        if let Some(until) = pause.until {
                            watch_server
                                .workers
                                .paused
                                .lock()
                                .expect("paused lock")
                                .insert(name, until);
                            go_on_at(&watch_server, sid.clone(), until);
                        }
                        let when = pause
                            .until
                            .map(|u| format!("; it goes on by itself after {}", local_time(u)))
                            .unwrap_or_else(|| "; send it a message to go on".into());
                        Ending::paused(format!("{}{when}", pause.reason), "limit", pause.until)
                    }
                    None => Ending::failed(format!(
                        "the worker ended ({ended}) without calling finish; its log is state/workers/{sid}.log"
                    )),
                },
            };
            watch_server.workers.end(&watch_server, &sid, ending);
        });
        Ok(())
    }

    /// A worker is done (or died): update the ticket, tell clients and the orchestrator.
    pub fn finish(&self, server: &Arc<Server>, worker: &str, state: WorkerState, summary: String) {
        self.end(
            server,
            worker,
            Ending {
                state,
                summary,
                pause: None,
                notify: true,
            },
        );
    }

    fn end(&self, server: &Arc<Server>, worker: &str, ending: Ending) {
        let Some(handle) = self.running.lock().expect("workers lock").remove(worker) else {
            return;
        };
        let Ending {
            state,
            summary,
            pause,
            notify,
        } = ending;
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
            let mut raw = set_status(&page.raw, status);
            if let Some((reason, until)) = &pause {
                raw = set_field(&raw, "paused_reason", reason);
                if let Some(until) = until {
                    raw = set_field(&raw, "paused_until", &until.to_string());
                }
            }
            if let Err(e) = server.write_page(&page.url, &raw, ChangedBy::Agent) {
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
        if !notify {
            return;
        }
        if let Ok(orchestrator) = server.sessions.get(&handle.parent) {
            let on_branch = branch
                .map(|b| {
                    format!(
                        " Its changes are on branch `{b}`; merge them with `merge_ticket` once \
                         the Result looks right."
                    )
                })
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

fn stop(handle: &WorkerHandle, reason: StopReason) {
    *handle.stop.lock().expect("stop lock") = Some(reason);
    handle.run.interrupt();
}

/// Resumes the paused worker `session` once `until` (unix seconds) has passed, if it is still
/// paused then.
fn go_on_at(server: &Arc<Server>, session: String, until: i64) {
    let server = server.clone();
    tokio::spawn(async move {
        let wait = (until - chrono::Utc::now().timestamp()).max(0) as u64 + 60;
        tokio::time::sleep(Duration::from_secs(wait)).await;
        let Ok(worker) = server.sessions.get(&session) else {
            return;
        };
        if worker.info().worker != Some(WorkerState::Paused) {
            return;
        }
        if let Err(e) = server
            .workers
            .resume(&server, &worker, GO_ON.into(), vec![])
            .await
        {
            tracing::warn!("resuming #{session} after its pause: {e}");
        }
    });
}

/// Worker sessions paused for `reason`, with their tickets.
fn paused_workers(server: &Server, reason: &str) -> Vec<(Arc<Session>, Page)> {
    server
        .sessions
        .list()
        .into_iter()
        .filter(|info| info.worker == Some(WorkerState::Paused))
        .filter_map(|info| {
            let page = ticket_page(server, info.ticket.as_deref()?).ok()?;
            let paused = page.frontmatter.get_str("status").as_deref() == Some("paused")
                && page.frontmatter.get_str("paused_reason").as_deref() == Some(reason);
            paused.then(|| Some((server.sessions.get(&info.id).ok()?, page)))?
        })
        .collect()
}

/// The ticket's project: its `project:`, else its orchestrator's.
fn ticket_project(server: &Server, page: &Page, parent_project: Option<String>) -> Option<String> {
    page.frontmatter
        .get_str("project")
        .filter(|p| !p.is_empty())
        .and_then(|p| resolve_project(&p, &server.config.projects.dir).ok())
        .or(parent_project)
}

/// The same for a worker session's ticket.
fn page_project(server: &Server, page: &Page, worker: &Session) -> Option<String> {
    let parent_project = worker
        .info()
        .parent
        .and_then(|p| server.sessions.get(&p).ok())
        .and_then(|p| p.info().project);
    ticket_project(server, page, parent_project)
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
        attachments: vec![],
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
