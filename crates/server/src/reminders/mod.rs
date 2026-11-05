//! Reminders: pages with `type: reminder` go off when their `due` date (and `time`) arrives.
//! The server checks the index every half minute and delivers each one once, recording
//! `notified:` in the page. A reminder written by an agent belongs to its session
//! (`session:`, filled in by the write tools) and is posted back there when it goes off.
//! A recurring one (`recurs: daily|weekly|monthly|<n>d`) stays open instead: going off moves
//! its `due:` to the next firing and appends a `- fired` line to its body. `due:` owns the
//! phase: the next firing counts on from the `due:` that fired, so editing `due:` moves the
//! whole series.

mod due;
mod recurrence;
mod scheduler;
mod types;

pub use due::{claim_reminder, due_reminders, mark_fired, mark_notified, roll_recurring};
pub use recurrence::Recurrence;
pub use scheduler::{check_reminders, run_reminders};
pub use types::DueReminder;
