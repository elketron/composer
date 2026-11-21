use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::harness::AcpAgent;
use crate::server::Server;
use crate::session::Session;

/// How long stopping the server waits for its agents to end.
const STOP_WAIT: Duration = Duration::from_secs(8);

/// The outside agents running sessions, one process per session that has a `harness`. One
/// starts with its session (or with its next turn, after a restart or a crash) and ends when the
/// session is deleted or the server stops.
#[derive(Default)]
pub struct AcpAgents {
    running: Mutex<HashMap<String, Arc<AcpAgent>>>,
}

impl AcpAgents {
    /// `session`'s agent: the running one, or one started now.
    pub fn start(
        &self,
        server: &Arc<Server>,
        session: &Arc<Session>,
    ) -> Result<Arc<AcpAgent>, String> {
        let mut running = self.running.lock().expect("agents lock");
        if let Some(agent) = running.get(session.id()) {
            return Ok(agent.clone());
        }
        let name = session
            .info()
            .harness
            .ok_or("this session runs on composer's own harness")?;
        let config = server
            .config()
            .acp
            .get(&name)
            .cloned()
            .ok_or_else(|| format!("no harness `{name}` in config.toml any more"))?;
        let agent = AcpAgent::launch(server, session, &config)?;
        running.insert(session.id().to_string(), agent.clone());
        // Gone when it ends by itself: the next turn starts it again.
        let (server, id, ended) = (
            Arc::downgrade(server),
            session.id().to_string(),
            agent.clone(),
        );
        tokio::spawn(async move {
            ended.ended().await;
            if let Some(server) = server.upgrade() {
                let mut running = server.acp.running.lock().expect("agents lock");
                if running.get(&id).is_some_and(|a| Arc::ptr_eq(a, &ended)) {
                    running.remove(&id);
                }
            }
        });
        Ok(agent)
    }

    /// Ends `session`'s agent, if it has one running.
    pub fn stop(&self, session: &str) {
        if let Some(agent) = self.running.lock().expect("agents lock").remove(session) {
            agent.stop();
        }
    }

    /// Ends every agent, and waits (a while) until they have.
    pub async fn stop_all(&self) {
        let agents: Vec<_> = self
            .running
            .lock()
            .expect("agents lock")
            .drain()
            .map(|(_, agent)| agent)
            .collect();
        for agent in &agents {
            agent.stop();
        }
        let all = futures::future::join_all(agents.iter().map(|agent| agent.ended()));
        let _ = tokio::time::timeout(STOP_WAIT, all).await;
    }
}

/// A turn of a session that runs on an outside agent: its messages (`text`) as one prompt.
pub async fn run_acp_turn(
    server: &Arc<Server>,
    session: &Arc<Session>,
    text: String,
    cancel: &CancellationToken,
) -> Result<Value, String> {
    let agent = server.acp.start(server, session)?;
    agent.prompt(server.sink(session), text, cancel).await
}
