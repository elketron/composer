//! CEF's work, on the main thread: a gpui task runs CEF's message loop and hands each browser
//! pane what CEF painted or asked for.

use std::cell::RefCell;
use std::time::Duration;

use gpui::{App, WeakEntity};

use crate::web_pane::WebPane;

/// While some pane is open: CEF work every 4 ms (smooth video, about 2 % of a core); otherwise
/// every 100 ms.
const BUSY: Duration = Duration::from_millis(4);
const IDLE: Duration = Duration::from_millis(100);

thread_local! {
    static PANES: RefCell<Vec<WeakEntity<WebPane>>> = const { RefCell::new(Vec::new()) };
}

pub fn register(pane: WeakEntity<WebPane>) {
    PANES.with(|panes| panes.borrow_mut().push(pane));
}

pub fn start(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let panes: Vec<_> = PANES.with(|p| {
                let mut p = p.borrow_mut();
                p.retain(|pane| pane.upgrade().is_some());
                p.clone()
            });
            // CEF's work runs even with no pane: its start-up continues in the loop, and a
            // browser made later on a CEF left unpumped never painted (ticket 0184).
            cef::do_message_loop_work();
            if panes.is_empty() {
                cx.background_executor().timer(IDLE).await;
                continue;
            }
            for pane in panes {
                let _ = pane.update(cx, |pane, cx| pane.after_pump(cx));
            }
            cx.background_executor().timer(BUSY).await;
        }
    })
    .detach();
}
