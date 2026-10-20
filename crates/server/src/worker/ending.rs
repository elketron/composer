use composer_protocol::WorkerState;

/// How a run ended, for the ticket, the session and the orchestrator.
pub struct Ending {
    pub state: WorkerState,
    pub summary: String,
    /// Paused: why (`limit`, `switch`, `shutdown`) and until when.
    pub pause: Option<(&'static str, Option<i64>)>,
    /// Tell the orchestrator (not while shutting down).
    pub notify: bool,
}

impl Ending {
    pub fn failed(summary: impl Into<String>) -> Self {
        Self {
            state: WorkerState::Failed,
            summary: summary.into(),
            pause: None,
            notify: true,
        }
    }

    pub fn paused(summary: String, reason: &'static str, until: Option<i64>) -> Self {
        Self {
            state: WorkerState::Paused,
            summary,
            pause: Some((reason, until)),
            notify: true,
        }
    }
}
