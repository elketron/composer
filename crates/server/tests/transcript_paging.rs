//! A history page must cost its page, not the whole session: the transcript is parsed once,
//! at open, and `history`/`replay` (the double parse on open) serve from memory. The counter
//! is process-wide and this test is the only one in its binary, so it is deterministic.

use std::sync::atomic::Ordering;

use composer_server::EventHub;
use composer_server::session::FULL_PARSES;
use composer_server::session::Transcript;

#[test]
fn a_page_does_not_reparse_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let hub = EventHub::new(64);
    let message = |n: usize| {
        serde_json::from_value(
            serde_json::json!({"role": "assistant", "content": format!("reply {n} {}", "x".repeat(600))}),
        )
        .unwrap()
    };
    let t = Transcript::open(&dir.path().join("events.jsonl"), 0).unwrap();
    for n in 0..2000 {
        t.emit(&hub, |seq| composer_protocol::ServerMessage::TurnStarted {
            session: "s".into(),
            seq,
        });
        t.emit(&hub, |seq| composer_protocol::ServerMessage::MessageDone {
            session: "s".into(),
            seq,
            message: message(n),
        });
        t.emit(&hub, |seq| composer_protocol::ServerMessage::TurnDone {
            session: "s".into(),
            seq,
            cancelled: false,
        });
    }
    assert_eq!(
        FULL_PARSES.load(Ordering::SeqCst),
        1,
        "open parses the file once"
    );
    let (page, more) = t.history(None, 50).unwrap();
    assert!(more);
    assert_eq!(
        page.len(),
        149,
        "50 messages from the cut message on, with their turns around it"
    );
    assert_eq!(
        FULL_PARSES.load(Ordering::SeqCst),
        1,
        "a history page must not parse the file again"
    );
    // What a live client attaching after seeing the rest of the session would get: one event.
    t.replay(Some(6000 - page.len() as u64), |events, _| events.len())
        .unwrap();
    assert_eq!(
        FULL_PARSES.load(Ordering::SeqCst),
        1,
        "the attach replay must not parse the file again"
    );
}
