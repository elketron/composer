//! What the chat has sent and is waiting to see come back: for now only the user review's
//! verdict (`pass`, `fail: …`). Clicking Pass puts its entry here and sends it; the entry *is*
//! the pressed state the card shows, and while it is here a second click adds nothing. It
//! leaves when the message comes back in the transcript (which, during a running turn, is when
//! the server starts the queued turn), or when the send fails.

use composer_client::ChatItem;

use crate::user_review::is_verdict;

/// A message sent and not seen back yet.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    session: String,
    text: String,
}

/// At most one entry per session.
#[derive(Debug, Default)]
pub struct Outbox {
    entries: Vec<Entry>,
}

impl Outbox {
    /// Put `text` in for `session`; `false` (nothing added, nothing to send) when that session
    /// already has one waiting.
    pub fn queue(&mut self, session: &str, text: &str) -> bool {
        if self.queued(session).is_some() {
            return false;
        }
        self.entries.push(Entry {
            session: session.to_string(),
            text: text.to_string(),
        });
        true
    }

    /// What `session` has waiting.
    pub fn queued(&self, session: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.session == session)
            .map(|e| e.text.as_str())
    }

    /// The send of `text` failed: it is not on its way, so it is not shown as such.
    pub fn failed(&mut self, session: &str, text: &str) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.session == session && e.text == text));
        self.entries.len() != before
    }

    /// A transcript item that just arrived in `session`: your verdict come back drains it.
    pub fn arrived(&mut self, session: &str, item: Option<&ChatItem>) -> bool {
        match item {
            Some(ChatItem::User {
                text, source: None, ..
            }) if is_verdict(text) => self.drop_session(session),
            _ => false,
        }
    }

    /// The review is no longer open in `session` (its history says so): nothing is waiting.
    pub fn settled(&mut self, session: &str) -> bool {
        self.drop_session(session)
    }

    fn drop_session(&mut self, session: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.session != session);
        self.entries.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(text: &str) -> ChatItem {
        ChatItem::User {
            text: text.into(),
            source: None,
            files: vec![],
        }
    }

    #[test]
    fn a_second_click_adds_nothing() {
        let mut outbox = Outbox::default();
        assert!(outbox.queue("s1", "pass"), "the first click sends");
        assert!(!outbox.queue("s1", "pass"), "the second does not");
        assert_eq!(outbox.queued("s1"), Some("pass"));
        assert_eq!(outbox.queued("s2"), None, "per session");
        assert!(outbox.queue("s2", "pass"));
    }

    #[test]
    fn drained_by_the_message_coming_back() {
        let mut outbox = Outbox::default();
        outbox.queue("s1", "pass");
        // Other things arriving (the running turn) leave it waiting.
        assert!(!outbox.arrived("s1", Some(&said("look at this"))));
        assert!(!outbox.arrived("s2", Some(&said("pass"))));
        assert_eq!(outbox.queued("s1"), Some("pass"));
        assert!(outbox.arrived("s1", Some(&said("pass"))));
        assert_eq!(outbox.queued("s1"), None);
        assert!(
            outbox.queue("s1", "pass"),
            "a later review can be passed again"
        );
    }

    #[test]
    fn a_failed_send_is_not_shown_as_sent() {
        let mut outbox = Outbox::default();
        outbox.queue("s1", "pass");
        assert!(!outbox.failed("s1", "something else"));
        assert!(outbox.failed("s1", "pass"));
        assert_eq!(outbox.queued("s1"), None);
        assert!(outbox.queue("s1", "pass"), "and can be clicked again");
    }
}
