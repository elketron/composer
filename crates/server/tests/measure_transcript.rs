//! Throwaway measurement (ticket 0053): does a history page cost track the page size or the
//! session size? Current code: reads and parses the whole file per request.

use std::path::Path;
use std::time::{Duration, Instant};

use composer_protocol::ServerMessage;
use composer_server::session::Transcript;
use composer_server::EventHub;

fn message(role: &str, text: String) -> composer_protocol::ChatMessage {
    serde_json::from_value(serde_json::json!({"role": role, "content": text})).unwrap()
}

fn build(path: &Path, messages: usize, hub: &EventHub) {
    let t = Transcript::open(path, 0).unwrap();
    for n in 0..messages {
        t.emit(hub, |seq| ServerMessage::TurnStarted { session: "s".into(), seq });
        t.emit(hub, |seq| ServerMessage::ToolResult {
            session: "s".into(),
            seq,
            call_id: "c".into(),
            result: "y".repeat(400),
            is_error: false,
        });
        t.emit(hub, |seq| ServerMessage::MessageDone {
            session: "s".into(),
            seq,
            message: message("assistant", format!("reply {n} {}", "x".repeat(600))),
        });
        t.emit(hub, |seq| ServerMessage::TurnDone { session: "s".into(), seq, cancelled: false });
    }
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn measure(label: &str, path: &Path, page: usize) {
    let hub = EventHub::new(64);
    let t = Transcript::open(path, 0).unwrap();
    let mut hist = Vec::new();
    let mut rep = Vec::new();
    let mut n = 0;
    for _ in 0..20 {
        let start = Instant::now();
        let (events, _) = t.history(None, page).unwrap();
        hist.push(start.elapsed());
        n = events.len();
        let start = Instant::now();
        t.replay(None, |e, _| e.len()).unwrap();
        rep.push(start.elapsed());
    }
    println!("{label}: page={page} file={} KB | history median {:?} ({} events) | replay median {:?}",
        std::fs::metadata(path).map(|m| m.len() / 1024).unwrap_or(0), median(hist), n, median(rep));
}

#[test]
fn cost_of_a_page() {
    let dir = tempfile::tempdir().unwrap();
    let hub = EventHub::new(64);
    let small = dir.path().join("small.jsonl");
    let big = dir.path().join("big.jsonl");
    let real = dir.path().join("real.jsonl");
    build(&small, 8, &hub);
    build(&big, 2000, &hub);
    let src = std::path::Path::new("/home/me/.composer/state/sessions/5464/events.jsonl");
    if src.exists() { std::fs::copy(src, &real).unwrap(); }
    for page in [10, 50, 200] {
        measure("small(8msg)", &small, page);
        measure("big(2000msg)", &big, page);
        if real.exists() { measure("real(5464)", &real, page); }
    }
}
