use std::process::Stdio;
use std::sync::Arc;

use crate::harness::{ChildProcess, Harness, HarnessRun, StartRequest};
use crate::server::Server;

/// Composer's own agent loop: a `composer worker` process that talks to this server over
/// WebSocket (slots, approvals, events and `finish`; see `worker::run_worker`).
pub struct OwnHarness;

impl Harness for OwnHarness {
    fn name(&self) -> &'static str {
        "composer"
    }

    fn start(
        &self,
        server: &Arc<Server>,
        request: StartRequest,
    ) -> Result<Arc<dyn HarnessRun>, String> {
        let listen = server
            .listen_addr()
            .ok_or("the server is not listening yet")?;
        let log = std::fs::File::create(&request.log).map_err(|e| e.to_string())?;
        let mut command = tokio::process::Command::new(&server.worker_exe);
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
        let process = ChildProcess::spawn(command)
            .map_err(|e| format!("starting {}: {e}", server.worker_exe.display()))?;
        Ok(Arc::new(process))
    }
}
