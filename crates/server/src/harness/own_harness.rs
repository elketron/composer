use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;

use crate::harness::{ChildProcess, Harness, HarnessRun, StartRequest, WorkerScope};
use crate::server::Server;

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
        if request.resume.is_some() {
            return Err("composer workers can't be resumed yet; open a new ticket".into());
        }
        let log = std::fs::File::create(&request.log).map_err(|e| e.to_string())?;
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
        for file in &request.files {
            command
                .arg("--attachment")
                .arg(format!("{}/{}", file.session, file.attachment.file));
        }
        let process = ChildProcess::spawn_in(command, scope)
            .map_err(|e| format!("starting {}: {e}", server.worker_exe.display()))?;
        Ok(Arc::new(process))
    }
}
