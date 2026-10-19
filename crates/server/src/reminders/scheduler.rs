//! Delivering due reminders: the check loop and what a delivery does.

use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDateTime, NaiveTime};
use composer_protocol::{ChangedBy, ChatMessage, MessageSource, Role, ServerMessage};

use crate::reminders::due::parse_time;
use crate::reminders::{DueReminder, ReminderAction, due_reminders, mark_notified};
use crate::server::Server;
use crate::session::submit;

/// How often the index is checked. Reminders are minute-precise at best, and a check is cheap.
const INTERVAL: Duration = Duration::from_secs(30);

/// Delivered this long after it was due counts as late (the server was off or asleep).
const LATE_AFTER_MINUTES: i64 = 15;

/// Checks for due reminders every half minute, for as long as the server runs.
pub async fn run_reminders(server: Arc<Server>) {
    let mut ticks = tokio::time::interval(INTERVAL);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        check_reminders(&server, chrono::Local::now().naive_local());
    }
}

/// Delivers every reminder due at `now` (local time) and returns them.
pub fn check_reminders(server: &Arc<Server>, now: NaiveDateTime) -> Vec<DueReminder> {
    let default_time = parse_time(&server.config.reminders.default_time)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"));
    // Collected first: writing the pages needs the index lock.
    let due = due_reminders(&server.index.read().expect("index lock"), now, default_time);
    due.into_iter()
        .filter(|reminder| deliver(server, reminder, now))
        .collect()
}

/// Records `notified:` in the page, then tells the clients (and the agent). Returns false if
/// the page couldn't be marked: delivering it anyway would repeat it every check.
fn deliver(server: &Arc<Server>, reminder: &DueReminder, now: NaiveDateTime) -> bool {
    let url = &reminder.url;
    let stamp = now.format("%Y-%m-%d %H:%M").to_string();
    let marked = server
        .workspace
        .read_page(url)
        .map_err(|e| e.to_string())
        .and_then(|page| mark_notified(&page.raw, &stamp).ok_or_else(|| "no frontmatter".into()))
        // Not the agent and not an app: the server's own bookkeeping, as for an editor's change.
        .and_then(|text| {
            server
                .write_page(url, &text, ChangedBy::External)
                .map_err(|e| e.to_string())
        });
    if let Err(e) = marked {
        tracing::warn!("reminder {url}: can't record it as notified, not delivering it: {e}");
        return false;
    }

    let due = reminder.due_at.format("%Y-%m-%d %H:%M").to_string();
    let session = match &reminder.action {
        ReminderAction::Notify => None,
        ReminderAction::Agent { session, agent } => {
            post_to_agent(server, reminder, &due, session.as_deref(), agent)
        }
    };
    tracing::info!("reminder {url} (due {due}) delivered");
    server.hub.send(ServerMessage::ReminderDue {
        url: url.to_string(),
        title: reminder.title.clone(),
        due,
        late: (now - reminder.due_at).num_minutes() > LATE_AFTER_MINUTES,
        session,
    });
    true
}

/// Posts the reminder into its session (or a new one); returns the session id.
fn post_to_agent(
    server: &Arc<Server>,
    reminder: &DueReminder,
    due: &str,
    session: Option<&str>,
    agent: &str,
) -> Option<String> {
    let existing = session.and_then(|id| server.sessions.get(id).ok());
    let session = match existing {
        Some(session) => session,
        None => match server.sessions.create(agent, None) {
            Ok(session) => {
                server.hub.send(ServerMessage::SessionUpdated {
                    session: session.info(),
                });
                session
            }
            Err(e) => {
                tracing::warn!("reminder {}: no session for the agent: {e}", reminder.url);
                return None;
            }
        },
    };
    let message = ChatMessage {
        role: Role::User,
        content: format!(
            "{} (due {due}). Open {} for what to do; when it's handled, set its status to done.",
            reminder.title, reminder.url
        ),
        thinking: None,
        tool_calls: vec![],
        pages: vec![],
        images: vec![],
        source: Some(MessageSource::Reminder {
            url: reminder.url.to_string(),
        }),
    };
    submit(server, &session, message);
    Some(session.id().to_string())
}
