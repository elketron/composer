//! CEF's work, on the main thread: a gpui task runs CEF's message loop when CEF asks for it
//! (`on_schedule_message_pump_work`, from any thread) and at least every so often, then hands
//! each browser pane what CEF painted or asked for.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Duration;

use futures::FutureExt;
use gpui::{App, WeakEntity};
use tokio::sync::mpsc;

use crate::web_pane::WebPane;

/// The longest the pump waits while a pane is open (CEF's own requests come sooner) …
const BUSY: Duration = Duration::from_millis(33);
/// … and while none is (CEF's start-up continues in the loop: a browser made later on an
/// unpumped CEF never painted).
const IDLE: Duration = Duration::from_millis(100);

thread_local! {
    static PANES: RefCell<Vec<WeakEntity<WebPane>>> = const { RefCell::new(Vec::new()) };
}

static REQUESTS: OnceLock<mpsc::UnboundedSender<i64>> = OnceLock::new();

pub fn register(pane: WeakEntity<WebPane>) {
    PANES.with(|panes| panes.borrow_mut().push(pane));
}

/// CEF wants its work run in `delay_ms` (called on CEF's threads).
pub fn schedule(delay_ms: i64) {
    if let Some(requests) = REQUESTS.get() {
        let _ = requests.send(delay_ms);
    }
}

/// How long to wait: the soonest of CEF's requests, never longer than `fallback`.
pub fn wait_for(requests: impl IntoIterator<Item = i64>, fallback: Duration) -> Duration {
    requests
        .into_iter()
        .map(|ms| Duration::from_millis(ms.max(0) as u64))
        .fold(fallback, Duration::min)
}

pub fn start(cx: &mut App) {
    let (sender, mut requests) = mpsc::unbounded_channel();
    let _ = REQUESTS.set(sender);
    cx.spawn(async move |cx| {
        let mut pending: Vec<i64> = Vec::new();
        loop {
            let started = std::time::Instant::now();
            cef::do_message_loop_work();
            if std::env::var_os("COMPOSER_CEF_TRACE").is_some() {
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                if ms > 3.0 {
                    eprintln!("[cef] message loop {ms:.1}ms");
                }
            }
            let panes: Vec<_> = PANES.with(|p| {
                let mut p = p.borrow_mut();
                p.retain(|pane| pane.upgrade().is_some());
                p.clone()
            });
            for pane in &panes {
                let _ = pane.update(cx, |pane, cx| pane.after_pump(cx));
            }
            // Requests made while working, then wait for the soonest (or a new one).
            while let Ok(ms) = requests.try_recv() {
                pending.push(ms);
            }
            let fallback = if panes.is_empty() { IDLE } else { BUSY };
            let wait = wait_for(pending.drain(..), fallback);
            if wait.is_zero() {
                continue;
            }
            let timer = cx.background_executor().timer(wait).fuse();
            let request = requests.recv().fuse();
            futures::pin_mut!(timer, request);
            futures::select! {
                _ = timer => {}
                ms = request => {
                    // A request sooner than the wait: honour its delay.
                    if let Some(ms) = ms.filter(|&ms| ms > 0) {
                        cx.background_executor().timer(Duration::from_millis(ms as u64)).await;
                    }
                }
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_soonest_request_or_the_fallback() {
        let fallback = Duration::from_millis(33);
        assert_eq!(wait_for([], fallback), fallback);
        assert_eq!(wait_for([10, 5, 40], fallback), Duration::from_millis(5));
        assert_eq!(
            wait_for([500], fallback),
            fallback,
            "never past the fallback"
        );
        assert_eq!(wait_for([-3], fallback), Duration::ZERO, "now");
    }
}
