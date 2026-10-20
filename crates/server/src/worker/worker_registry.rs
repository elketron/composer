use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use composer_protocol::{
    ChangedBy, ChatMessage, MessageSource, Role, ServerMessage, TurnState, WorkerState,
};

use crate::agent::AgentDefinition;
use crate::server::Server;
use crate::session::{Session, submit};
use crate::tools::resolve_project;
use crate::worker::WorkerHandle;
use crate::worker::ticket::{set_status, ticket_url};

/// Worker processes started by this server, by worker session id.
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

    /// Starts a worker process on ticket `id` for the orchestrator `parent`, running
    /// `agent` (else the ticket's `agent:`, else `worker`). Returns the worker's session id.
    pub fn spawn(
        &self,
        server: &Arc<Server>,
        parent: &Arc<Session>,
        id: &str,
        agent: Option<&str>,
    ) -> Result<String, String> {
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
        let project = page
            .frontmatter
            .get_str("project")
            .filter(|p| !p.is_empty())
            .and_then(|p| resolve_project(&p, &server.config.projects.dir).ok())
            .or_else(|| parent.info().project);
        let listen = server
            .listen_addr()
            .ok_or("the server is not listening yet")?;

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
                &set_status(&page.raw, "claimed"),
                ChangedBy::Agent,
            )
            .map_err(|e| e.to_string())?;

        let logs = server.home.state().join("workers");
        std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
        let log =
            std::fs::File::create(logs.join(format!("{sid}.log"))).map_err(|e| e.to_string())?;
        let mut command = tokio::process::Command::new(&server.worker_exe);
        command
            .args([
                "worker",
                "--session",
                &sid,
                "--ticket",
                id,
                "--agent",
                &agent,
            ])
            .args(["--parent", &format!("http://{listen}")])
            .env("COMPOSER_HOME", server.home.root())
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        if let Some(project) = &project {
            command.args(["--project", project]);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("starting {}: {e}", server.worker_exe.display()))?;

        self.running.lock().expect("workers lock").insert(
            sid.clone(),
            WorkerHandle {
                ticket: id.to_string(),
                parent: parent.id().to_string(),
            },
        );
        server.hub.send(ServerMessage::WorkerStatus {
            worker: sid.clone(),
            ticket: id.to_string(),
            parent: parent.id().to_string(),
            state: WorkerState::Starting,
        });

        // If the process ends without `finish`, the ticket fails and the orchestrator hears why.
        let (watch_server, watch_sid) = (server.clone(), sid.clone());
        tokio::spawn(async move {
            let status = child.wait().await;
            let still_running = watch_server
                .workers
                .running
                .lock()
                .expect("workers lock")
                .contains_key(&watch_sid);
            if still_running {
                let code = status
                    .ok()
                    .and_then(|s| s.code())
                    .map_or("killed".into(), |c| format!("exit {c}"));
                let summary = format!(
                    "the worker process ended ({code}) without calling finish; its log is state/workers/{watch_sid}.log"
                );
                watch_server.workers.finish(
                    &watch_server,
                    &watch_sid,
                    WorkerState::Failed,
                    summary,
                );
            }
        });
        Ok(sid)
    }

    /// A worker is done (or died): update the ticket, tell clients and the orchestrator.
    pub fn finish(&self, server: &Arc<Server>, worker: &str, state: WorkerState, summary: String) {
        let Some(handle) = self.running.lock().expect("workers lock").remove(worker) else {
            return;
        };
        let status = if state == WorkerState::Done {
            "done"
        } else {
            "failed"
        };
        if let Ok(url) = ticket_url(&handle.ticket)
            && let Ok(page) = server.workspace.read_page(&url)
            && let Err(e) =
                server.write_page(&page.url, &set_status(&page.raw, status), ChangedBy::Agent)
        {
            tracing::error!("updating ticket {}: {e}", handle.ticket);
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
            let content = format!(
                "[worker #{worker} finished ticket {ticket}: {status}]\n{summary}\n\nIts Result is in site://orchestration/tickets/{ticket}.md.",
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
