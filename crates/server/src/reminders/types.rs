use chrono::NaiveDateTime;
use composer_core::SiteUrl;

/// What a due reminder does besides notifying every client (`action:` in its frontmatter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderAction {
    Notify,
    /// Also posts it into a session for the agent: `session:`, else a new one with `agent:`.
    Agent {
        session: Option<String>,
        agent: String,
    },
}

/// A reminder whose time has come and that hasn't been delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueReminder {
    pub url: SiteUrl,
    pub title: String,
    /// Local time.
    pub due_at: NaiveDateTime,
    pub action: ReminderAction,
}
