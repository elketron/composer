//! Reminders: pages with `type: reminder` go off when their `due` date (and `time`) arrives.
//! The server checks the index every half minute and delivers each one once, recording
//! `notified:` in the page.

mod due;
mod scheduler;
mod types;

pub use due::{due_reminders, mark_notified};
pub use scheduler::{check_reminders, run_reminders};
pub use types::{DueReminder, ReminderAction};
