//! What the chat has sent and is waiting to see come back: for now only the user review's
//! verdict (`pass`, `fail: …`). An entry answers one review (a session and the call id of the
//! `open` that started the review), at most one per review: clicking Pass puts it here and sends
//! it, the entry *is* the pressed state the card shows, and while it is here a second click adds
//! nothing. It leaves when that review is no longer the one open (your verdict came back after
//! it, or another review was opened since), when the send fails, or, after a reconnect, when the
//! session turns out idle without it having come back (the server's inbox lost it).

use std::collections::HashMap;

/// A review: its session and the call id of the `open` that started it.
type Review = (String, String);

#[derive(Debug, Default)]
pub struct Outbox {
    /// The verdict sent for each review.
    sent: HashMap<Review, String>,
    /// Sent before the connection dropped: the server may have lost it.
    unconfirmed: HashMap<Review, String>,
}

impl Outbox {
    /// Put `text` in as the answer to `review` in `session`; `false` (nothing added, nothing to
    /// send) when that review already has one.
    pub fn queue(&mut self, session: &str, review: &str, text: &str) -> bool {
        if self.queued(session, review).is_some() {
            return false;
        }
        self.sent
            .insert((session.into(), review.into()), text.to_string());
        true
    }

    /// The verdict sent for `review` in `session` and not back yet.
    pub fn queued(&self, session: &str, review: &str) -> Option<&str> {
        let key = (session.to_string(), review.to_string());
        self.sent
            .get(&key)
            .or_else(|| self.unconfirmed.get(&key))
            .map(String::as_str)
    }

    /// The send answering `review` failed: it is not on its way, so it is not shown as such.
    pub fn failed(&mut self, session: &str, review: &str) -> bool {
        let key = (session.to_string(), review.to_string());
        let sent = self.sent.remove(&key).is_some();
        self.unconfirmed.remove(&key).is_some() || sent
    }

    /// What `session`'s transcript says now: `open` is the review still waiting for a verdict.
    /// Every other review of the session has been answered (or replaced): its entry goes.
    pub fn sync(&mut self, session: &str, open: Option<&str>) -> bool {
        let stale = |(s, r): &Review| s == session && Some(r.as_str()) != open;
        let before = self.sent.len() + self.unconfirmed.len();
        self.sent.retain(|k, _| !stale(k));
        self.unconfirmed.retain(|k, _| !stale(k));
        self.sent.len() + self.unconfirmed.len() != before
    }

    /// The connection came back. The server keeps a queued message in memory only, so what was
    /// sent before may be gone; it is shown as sent until the session says otherwise.
    pub fn reconnected(&mut self) {
        self.unconfirmed.extend(self.sent.drain());
    }

    /// `session` is idle. What it still owes from before a reconnect is not coming: a message
    /// queued behind a turn arrives right as that turn ends, and one the server still held would
    /// have kept it busy.
    pub fn idle(&mut self, session: &str) -> bool {
        let before = self.unconfirmed.len();
        self.unconfirmed.retain(|(s, _), _| s != session);
        self.unconfirmed.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_entry_per_review() {
        let mut outbox = Outbox::default();
        assert!(outbox.queue("s1", "r1", "pass"), "the first click sends");
        assert!(!outbox.queue("s1", "r1", "pass"), "the second does not");
        assert_eq!(outbox.queued("s1", "r1"), Some("pass"));
        assert_eq!(outbox.queued("s2", "r1"), None, "per session");
        assert_eq!(outbox.queued("s1", "r2"), None, "per review");
        assert!(outbox.queue("s1", "r2", "pass"));
    }

    #[test]
    fn drained_when_its_review_is_no_longer_open() {
        let mut outbox = Outbox::default();
        outbox.queue("s1", "r1", "pass");
        outbox.queue("s2", "r1", "pass");
        assert!(!outbox.sync("s1", Some("r1")), "still the open review");
        assert!(outbox.sync("s1", Some("r2")), "another review opened since");
        assert_eq!(outbox.queued("s1", "r1"), None);
        assert_eq!(
            outbox.queued("s2", "r1"),
            Some("pass"),
            "other sessions keep theirs"
        );
        assert!(outbox.sync("s2", None), "answered");
        assert_eq!(outbox.queued("s2", "r1"), None);
    }

    #[test]
    fn a_failed_send_is_not_shown_as_sent() {
        let mut outbox = Outbox::default();
        outbox.queue("s1", "r1", "pass");
        assert!(!outbox.failed("s1", "r2"));
        assert!(!outbox.failed("s2", "r1"));
        assert!(outbox.failed("s1", "r1"));
        assert_eq!(outbox.queued("s1", "r1"), None);
        assert!(outbox.queue("s1", "r1", "pass"), "and can be clicked again");
    }

    #[test]
    fn lost_in_a_reconnect_it_goes_when_the_session_is_idle() {
        let mut outbox = Outbox::default();
        outbox.queue("s1", "r1", "pass");
        outbox.queue("s2", "r1", "pass");
        outbox.reconnected();
        assert_eq!(
            outbox.queued("s1", "r1"),
            Some("pass"),
            "shown until we know"
        );
        assert!(!outbox.queue("s1", "r1", "pass"));
        assert!(outbox.idle("s1"));
        assert_eq!(outbox.queued("s1", "r1"), None, "the server lost it");
        assert_eq!(outbox.queued("s2", "r1"), Some("pass"), "s2 hasn't said");
        // Sent after the reconnect: idle doesn't touch it (it is the turn about to start).
        outbox.queue("s1", "r1", "pass");
        assert!(!outbox.idle("s1"));
        assert_eq!(outbox.queued("s1", "r1"), Some("pass"));
    }
}
