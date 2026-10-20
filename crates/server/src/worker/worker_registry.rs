use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use composer_protocol::{
    ChangedBy, ChatMessage, MessageSource, Role, ServerMessage, TurnState, WorkerState,
};

use crate::agent::AgentDefinition;
use crate::harness::{Harness, OwnHarness, StartRequest};
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
        vec![Arc::new(OwnHarness)]
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

    /// Starts a worker on ticket `id` for the orchestrator `parent`, running
    /// `agent` (else the ticket's `agent:`, else `worker`). Returns the worker's session id and,
    /// for a git project, the worktree it works in.
    pub fn spawn(
        &self,
        server: &Arc<Server>,
        parent: &Arc<Session>,
        id: &str,
        agent: Option<&str>,
    ) -> Result<(String, Option<Worktree>), String> {
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

        let url = ticket_url(id).map_err(|e| e.to_string())?;
        let page = server.workspace.read_page(&url).map_err(|_| {
            format!("no ticket {id}; create it with `run orchestration new-ticket <title>`")
        })?;
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
        let definition =
            AgentDefinition::load(&server.workspace, &agent).map_err(|e| e.to_string())?;
        if !definition.is_worker() {
            let index = server.index.read().expect("index lock");
            let workers: Vec<String> = AgentDefinition::workers(&server.workspace, &index)
                .into_iter()
                .map(|w| w.name)
                .collect();
            return Err(format!(
                "`{agent}` is not a worker agent (its page needs `role: worker`); worker agents: {}",
                if workers.is_empty() {
                    "none".to_string()
                } else {
                    workers.join(", ")
                }
            ));
        }
        let harnesses = Self::harnesses();
        let wanted = page
            .frontmatter
            .get_str("harness")
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| harnesses[0].name().to_string());
        let harness = harnesses
            .iter()
            .find(|h| h.name() == wanted)
            .cloned()
            .ok_or_else(|| {
                let names: Vec<&str> = harnesses.iter().map(|h| h.name()).collect();
                format!(
                    "ticket {id} asks for harness `{wanted}`; available: {}",
                    names.join(", ")
                )
            })?;
        let project = page
            .frontmatter
            .get_str("project")
            .filter(|p| !p.is_empty())
            .and_then(|p| resolve_project(&p, &server.config.projects.dir).ok())
            .or_else(|| parent.info().project);
        if server.listen_addr().is_none() {
            return Err("the server is not listening yet".into());
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
        let project = worktree
            .as_ref()
            .map(|w| w.path.to_string_lossy().to_string())
            .or(project);

        let session = server
            .sessions
            .create(&agent, project.clone())
            .map_err(|e| e.to_string())?;
        let sid = session.id().to_string();
        session.update_info(&server.hub, |info| {
            info.ticket = Some(id.to_string());
            info.parent = Some(parent.id().to_string());
            info.worker = Some(WorkerState::Starting);
            info.preview = Some(format!("ticket {id}: {}", page.title()));
        });
        server
            .write_page(
                &page.url,
                &match &worktree {
                    Some(w) => set_field(&set_status(&page.raw, "claimed"), "branch", &w.branch),
                    None => set_status(&page.raw, "claimed"),
                },
                ChangedBy::Agent,
            )
            .map_err(|e| e.to_string())?;

        let logs = server.home.state().join("workers");
        std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
        let request = StartRequest {
            session: sid.clone(),
            ticket: id.to_string(),
            agent: agent.clone(),
            project,
            log: logs.join(format!("{sid}.log")),
        };
        let run = match harness.start(server, request) {
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
                ticket: id.to_string(),
                parent: parent.id().to_string(),
                harness: harness.name(),
                run: run.clone(),
                cancelled: cancelled.clone(),
            },
        );
        server.hub.send(ServerMessage::WorkerStatus {
            worker: sid.clone(),
            ticket: id.to_string(),
            parent: parent.id().to_string(),
            state: WorkerState::Starting,
        });

        // If the run ends without `finish`, the ticket fails and the orchestrator hears why.
        let (watch_server, watch_sid) = (server.clone(), sid.clone());
        tokio::spawn(async move {
            let ended = run.wait().await;
            if watch_server.workers.get(&watch_sid).is_some() {
                let summary = if cancelled.load(Ordering::SeqCst) {
                    "cancelled by the user".to_string()
                } else {
                    format!(
                        "the worker ended ({ended}) without calling finish; its log is state/workers/{watch_sid}.log"
                    )
                };
                watch_server.workers.finish(
                    &watch_server,
                    &watch_sid,
                    WorkerState::Failed,
                    summary,
                );
            }
        });
        Ok((sid, worktree))
    }

    /// A worker is done (or died): update the ticket, tell clients and the orchestrator.
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
        let status = if state == WorkerState::Done {
            "done"
        } else {
            "failed"
        };
        let mut branch = None;
        if let Ok(url) = ticket_url(&handle.ticket)
            && let Ok(page) = server.workspace.read_page(&url)
        {
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
            let message = ChatMessage {
                role: Role::User,
                content,
                thinking: None,
                tool_calls: vec![],
                pages: vec![],
                images: vec![],
                source: Some(MessageSource::Worker {
                    session: worker.to_string(),
                }),
            };
            submit(server, &orchestrator, message);
        }
    }
}
