use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;

use crate::harness::{ChildProcess, Harness, HarnessRun, StartRequest, WorkerScope};
use crate::server::Server;
use crate::worker::ticket::set_ticket_fields;

/// Composer's own agent loop: a `composer worker` process that talks to this server over
/// WebSocket (slots, approvals, events and `finish`; see `worker::run_worker`).
pub struct OwnHarness;

#[async_trait]
impl Harness for OwnHarness {
    fn name(&self) -> &'static str {
        "composer"
    }

    fn external(&self) -> bool {
        false
    }

    fn default_model(&self, server: &Server) -> Option<String> {
        Some(server.provider.model().to_string())
    }

    /// The provider's one model.
    fn chooses_model(&self) -> bool {
        false
    }

    async fn start(
        &self,
        server: &Arc<Server>,
        request: StartRequest,
    ) -> Result<Arc<dyn HarnessRun>, String> {
        let listen = server
            .listen_addr()
            .ok_or("the server is not listening yet")?;
        // Kept across runs: a resumed one goes on in the same log.
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&request.log)
            .map_err(|e| e.to_string())?;
        let (mut command, scope) = WorkerScope::command(
            server.worker_exe.as_os_str(),
            &request.session,
            &server.config().agent.worker_memory_max,
        );
        command
            .args([
                "worker",
                "--session",
                &request.session,
                "--ticket",
                &request.ticket,
                "--agent",
                &request.agent,
            ])
            .args(["--parent", &format!("http://{listen}")])
            .env("COMPOSER_HOME", server.home.root())
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        if let Some(project) = &request.project {
            command.args(["--project", project]);
        }
        match &request.resume {
            // Its history is in the worker's state; the message (and its files) goes on from it.
            Some(resume) => {
                command.arg("--resume").arg(&resume.message);
                for path in &resume.files {
                    if let Some(file) = path.file_name().and_then(|f| f.to_str()) {
                        command
                            .arg("--attachment")
                            .arg(format!("{}/{file}", request.session));
                    }
                }
            }
            None => {
                for file in &request.files {
                    command
                        .arg("--attachment")
                        .arg(format!("{}/{}", file.session, file.attachment.file));
                }
                // The ticket says which session to resume, as an external run's does.
                set_ticket_fields(
                    server,
                    &request.ticket,
                    &[("harness_session", &request.session)],
                )?;
            }
        }
        let process = ChildProcess::spawn_in(command, scope)
            .map_err(|e| format!("starting {}: {e}", server.worker_exe.display()))?;
        Ok(Arc::new(process))
    }
}
