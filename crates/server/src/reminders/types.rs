use chrono::NaiveDateTime;
use composer_core::SiteUrl;

/// A reminder whose time has come and that hasn't been delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueReminder {
    pub url: SiteUrl,
    pub title: String,
    /// Local time.
    pub due_at: NaiveDateTime,
    /// The session it belongs to (`session:`; the write tools fill in the writer's).
    pub session: Option<String>,
    /// Who gets a new session for it when it has none (`agent:`, default `default`).
    pub agent: String,
}
