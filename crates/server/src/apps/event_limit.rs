use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Events an app may report to a session per minute (ticket 0175): more is refused, so a looping
/// app can't flood the agent's context.
const PER_MINUTE: usize = 20;
const WINDOW: Duration = Duration::from_secs(60);

/// How many events each (session, app) reported in the last minute.
#[derive(Default)]
pub struct EventLimit {
    recent: Mutex<HashMap<(String, String), VecDeque<Instant>>>,
}

impl EventLimit {
    /// Counts one event of `app` for `session`: `false` when it is over the limit.
    pub fn allow(&self, session: &str, app: &str) -> bool {
        let mut recent = self.recent.lock().expect("event limit lock");
        let times = recent
            .entry((session.to_string(), app.to_string()))
            .or_default();
        while times.front().is_some_and(|t| t.elapsed() > WINDOW) {
            times.pop_front();
        }
        if times.len() >= PER_MINUTE {
            return false;
        }
        times.push_back(Instant::now());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_session_and_app() {
        let limit = EventLimit::default();
        for _ in 0..PER_MINUTE {
            assert!(limit.allow("s", "board"));
        }
        assert!(!limit.allow("s", "board"));
        assert!(limit.allow("s", "cad") && limit.allow("t", "board"));
    }
}
