//! Isolated CEF/GPUI frame-retention soak; no Composer home, server, session or token.
//! `COMPOSER_CEF_GPU=1 cargo run --example frame_soak`
//! Runs four 15-second phases, then exits. Sample its process tree's RSS/PSS externally.

#[path = "../src/browser_frame.rs"]
mod browser_frame;
#[path = "../src/browser_pump.rs"]
mod browser_pump;
#[path = "../src/cef_runtime.rs"]
mod cef_runtime;
#[allow(dead_code)]
#[path = "../src/dom_keys.rs"]
mod dom_keys;
#[allow(dead_code)]
#[path = "../src/view_urls.rs"]
mod view_urls;
#[allow(dead_code)]
#[path = "../src/web_pane.rs"]
mod web_pane;

use std::time::Duration;

use base64::Engine;
use gpui::{
    App, AppContext, Bounds, Context, Entity, Render, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use web_pane::WebPane;

const LABELS: [&str; 4] = [
    "foreground animation",
    "not drawn, CEF deliberately still painting (bounded-buffer defense)",
    "hidden via was_hidden (background-tab policy)",
    "foreground animation resumed",
];

struct FrameSoak {
    pane: Entity<WebPane>,
    phase: usize,
}

impl FrameSoak {
    fn new(cx: &mut Context<Self>) -> Self {
        // Repaint a physical 1080p canvas every animation frame. No network or filesystem IO.
        let html = r#"<!doctype html><body style="margin:0;background:#121317;color:#d5d8de;font:16px monospace">
<canvas width="1920" height="1080" style="width:100%;height:100%"></canvas>
<script>const c=document.querySelector('canvas'),x=c.getContext('2d');let n=0;
function draw(){x.fillStyle=n++%2?'#19222f':'#243247';x.fillRect(0,0,c.width,c.height);
x.fillStyle='#8fb3e8';x.fillRect(n*17%1700,80,220,900);requestAnimationFrame(draw)}draw();</script>"#;
        let url = format!(
            "data:text/html;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(html)
        );
        let pane = cx.new(|cx| WebPane::new(&url, "http://127.0.0.1:1", None, cx));
        pane.update(cx, |pane, cx| pane.set_visible(true, cx));
        eprintln!("frame-soak phase 0: {}", LABELS[0]);
        cx.spawn(async move |this, cx| {
            for (phase, label) in LABELS.iter().enumerate().skip(1) {
                cx.background_executor()
                    .timer(Duration::from_secs(15))
                    .await;
                if this
                    .update(cx, |soak, cx| {
                        soak.phase = phase;
                        soak.pane
                            .update(cx, |pane, cx| pane.set_visible(phase != 2, cx));
                        eprintln!("frame-soak phase {phase}: {label}");
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            cx.background_executor()
                .timer(Duration::from_secs(15))
                .await;
            eprintln!("frame-soak complete");
            cx.update(|cx| cx.quit());
        })
        .detach();
        Self { pane, phase: 0 }
    }
}

impl Render for FrameSoak {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x121317))
            .text_color(rgb(0xd5d8de))
            .child(if self.phase == 0 || self.phase == 3 {
                self.pane.clone().into_any_element()
            } else {
                div().child(LABELS[self.phase]).into_any_element()
            })
    }
}

pub fn trace(_step: &str) {}

fn main() {
    cef_runtime::run_helper_process();
    let cache = cef_runtime::start().expect("CEF runtime installed");
    gpui_platform::application().run(|cx: &mut App| {
        browser_pump::start(cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("composer-frame-soak".into()),
                ..Default::default()
            },
            |_, cx| cx.new(FrameSoak::new),
        )
        .expect("soak window");
        cx.activate(true);
    });
    cef_runtime::shutdown(cache);
}
