//! A browser pane: CEF renders offscreen and the pane draws its frames; mouse, wheel, keys and IME
//! go back through the browser host. Links to the workspace's pages open as tabs (events), popups
//! never get windows. Main thread only: CEF runs on the external pump (`browser_pump`).
//! Rule from the spike: never call into the browser host while holding `Shared`'s borrow (CEF
//! calls back into the handlers synchronously).

use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use cef::{rc::Rc as _, *};
use gpui::{
    App, Bounds, Context, CursorStyle, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, KeyDownEvent, KeyUpEvent, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, RenderImage, ScrollDelta,
    ScrollEvent, UTF16Selection, Window, canvas, div, point, prelude::*, px,
};
use smallvec::SmallVec;

use crate::browser_pump;
use crate::dom_keys::codes;
use crate::view_urls::{external, page_for};

/// What CEF's callbacks write and the pane reads. Main thread only.
#[derive(Default)]
pub struct Shared {
    /// Logical size of the pane, as CEF's view rect.
    pub size: (i32, i32),
    pub scale: f32,
    /// Latest view frame, BGRA, physical pixels.
    pub frame: Option<(u32, u32, Vec<u8>)>,
    pub popup_rect: Option<Rect>,
    pub popup_frame: Option<(u32, u32, Vec<u8>)>,
    pub dirty: bool,
    pub cursor: CursorStyle,
    /// Pages the user asked for (links, popups), for the window to open as tabs.
    pub open_pages: Vec<String>,
    /// Links that leave the workspace, for the OS to open.
    pub open_outside: Vec<String>,
    /// Wheel delta waiting for the next pump (batched: a trackpad sends hundreds of events a
    /// second; one CEF wheel call per pump keeps the main thread free and scrolls smoothly).
    pub wheel: (i32, i32),
    pub wheel_at: Option<(Point<Pixels>, Modifiers)>,
}

#[derive(Clone)]
struct Handler {
    shared: Rc<RefCell<Shared>>,
    /// The workspace base, for telling page links from the rest.
    base: Rc<str>,
}

wrap_render_handler! {
    struct RenderHandlerBuilder {
        handler: Handler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let s = self.handler.shared.borrow();
                rect.width = s.size.0.max(1);
                rect.height = s.size.1.max(1);
            }
        }

        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> ::std::os::raw::c_int {
            if let Some(info) = info {
                info.device_scale_factor = self.handler.shared.borrow().scale.max(1.0);
                return 1;
            }
            0
        }

        fn on_popup_show(&self, _browser: Option<&mut Browser>, show: ::std::os::raw::c_int) {
            let mut s = self.handler.shared.borrow_mut();
            if show == 0 {
                s.popup_rect = None;
                s.popup_frame = None;
            }
            s.dirty = true;
        }

        fn on_popup_size(&self, _browser: Option<&mut Browser>, rect: Option<&Rect>) {
            if let Some(rect) = rect {
                self.handler.shared.borrow_mut().popup_rect = Some(rect.clone());
            }
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let len = (width * height * 4) as usize;
            let started = std::time::Instant::now();
            // SAFETY: CEF guarantees `buffer` holds width*height BGRA pixels for this call.
            let bytes = unsafe { std::slice::from_raw_parts(buffer, len) }.to_vec();
            if std::env::var_os("COMPOSER_CEF_TRACE").is_some() {
                crate::trace(&format!(
                    "cef paint {}x{} copy {:.2}ms",
                    width,
                    height,
                    started.elapsed().as_secs_f64() * 1000.0
                ));
            }
            let mut s = self.handler.shared.borrow_mut();
            let frame = Some((width as u32, height as u32, bytes));
            if type_ == PaintElementType::from(sys::cef_paint_element_type_t::PET_POPUP) {
                s.popup_frame = frame;
            } else {
                s.frame = frame;
            }
            s.dirty = true;
        }
    }
}

wrap_display_handler! {
    struct DisplayHandlerBuilder {
        handler: Handler,
    }

    impl DisplayHandler {
        fn on_cursor_change(
            &self,
            _browser: Option<&mut Browser>,
            _cursor: ::std::os::raw::c_ulong,
            type_: CursorType,
            _custom: Option<&CursorInfo>,
        ) -> ::std::os::raw::c_int {
            use sys::cef_cursor_type_t::*;
            let raw: &sys::cef_cursor_type_t = type_.as_ref();
            let style = match *raw {
                CT_HAND => CursorStyle::PointingHand,
                CT_IBEAM => CursorStyle::IBeam,
                CT_CROSS => CursorStyle::Crosshair,
                CT_EASTWESTRESIZE | CT_EASTRESIZE | CT_WESTRESIZE | CT_COLUMNRESIZE => CursorStyle::ResizeLeftRight,
                CT_NORTHSOUTHRESIZE | CT_NORTHRESIZE | CT_SOUTHRESIZE | CT_ROWRESIZE => CursorStyle::ResizeUpDown,
                CT_NOTALLOWED => CursorStyle::OperationNotAllowed,
                CT_GRAB => CursorStyle::OpenHand,
                CT_GRABBING => CursorStyle::ClosedHand,
                _ => CursorStyle::Arrow,
            };
            let mut s = self.handler.shared.borrow_mut();
            s.cursor = style;
            s.dirty = true;
            1
        }
    }
}

wrap_request_handler! {
    struct RequestHandlerBuilder {
        handler: Handler,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            // Only links the user follows in the main frame; the tab's own load goes through.
            let main = frame.is_some_and(|f| f.is_main() != 0);
            let (Some(request), true, true) = (request, main, user_gesture != 0) else { return 0 };
            let url = CefStringUtf16::from(&request.url()).to_string();
            if let Some(page) = page_for(&url, &self.handler.base) {
                self.handler.shared.borrow_mut().open_pages.push(page);
                return 1;
            }
            if external(&url, &self.handler.base) {
                self.handler.shared.borrow_mut().open_outside.push(url);
                return 1;
            }
            0
        }
    }
}

wrap_life_span_handler! {
    struct LifeSpanHandlerBuilder {
        handler: Handler,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // No windows of their own: a page opens as a tab, anything else waits for 0186
            // (opening with the OS).
            let url = target_url.map(|u| u.to_string()).unwrap_or_default();
            if let Some(page) = page_for(&url, &self.handler.base) {
                self.handler.shared.borrow_mut().open_pages.push(page);
            } else if external(&url, &self.handler.base) {
                self.handler.shared.borrow_mut().open_outside.push(url);
            }
            1
        }
    }
}

wrap_client! {
    struct ClientBuilder {
        render: RenderHandler,
        display: DisplayHandler,
        request: RequestHandler,
        life_span: LifeSpanHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<cef::RenderHandler> {
            Some(self.render.clone())
        }
        fn display_handler(&self) -> Option<cef::DisplayHandler> {
            Some(self.display.clone())
        }
        fn request_handler(&self) -> Option<cef::RequestHandler> {
            Some(self.request.clone())
        }
        fn life_span_handler(&self) -> Option<cef::LifeSpanHandler> {
            Some(self.life_span.clone())
        }
    }
}

pub struct WebPane {
    pub shared: Rc<RefCell<Shared>>,
    browser: Option<Browser>,
    focus: FocusHandle,
    origin: Point<Pixels>,
    image: Option<Arc<RenderImage>>,
    popup_image: Option<Arc<RenderImage>>,
    /// Pending images replaced this frame; dropped from the atlas on the next paint.
    stale: Vec<Arc<RenderImage>>,
    marked: Option<String>,
    buttons_down: u32,
}

/// What a browser pane asks of its window.
pub enum WebEvent {
    /// A link to a page: open it as a tab.
    OpenPage(String),
    /// A link outside the workspace: the OS opens it.
    OpenOutside(String),
}

impl EventEmitter<WebEvent> for WebPane {}

impl WebPane {
    /// A browser showing `url`; links to pages of the workspace at `base` become [`WebEvent`]s.
    pub fn new(url: &str, base: &str, cx: &mut Context<Self>) -> Self {
        let shared = Rc::new(RefCell::new(Shared {
            size: (800, 600),
            scale: 1.0,
            ..Default::default()
        }));
        let handler = Handler {
            shared: shared.clone(),
            base: base.into(),
        };
        let mut client = ClientBuilder::new(
            RenderHandlerBuilder::new(handler.clone()),
            DisplayHandlerBuilder::new(handler.clone()),
            RequestHandlerBuilder::new(handler.clone()),
            LifeSpanHandlerBuilder::new(handler),
        );
        let window_info = WindowInfo {
            windowless_rendering_enabled: 1,
            ..Default::default()
        };
        let settings = BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };
        let browser = browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut client),
            Some(&url.into()),
            Some(&settings),
            None,
            None,
        );
        browser_pump::register(cx.weak_entity());
        Self {
            shared,
            browser,
            focus: cx.focus_handle(),
            origin: point(px(0.), px(0.)),
            image: None,
            popup_image: None,
            stale: Vec::new(),
            marked: None,
            buttons_down: 0,
        }
    }

    fn host(&self) -> Option<BrowserHost> {
        self.browser.as_ref().and_then(|b| b.host())
    }

    /// After CEF's work: new frames become images, links to pages become events.
    pub fn after_pump(&mut self, cx: &mut Context<Self>) {
        let started = std::time::Instant::now();
        let (pages, outside) = {
            let mut s = self.shared.borrow_mut();
            (
                std::mem::take(&mut s.open_pages),
                std::mem::take(&mut s.open_outside),
            )
        };
        for page in pages {
            cx.emit(WebEvent::OpenPage(page));
        }
        for url in outside {
            cx.emit(WebEvent::OpenOutside(url));
        }
        let mut s = self.shared.borrow_mut();
        let wheel = std::mem::take(&mut s.wheel);
        let wheel_at = s.wheel_at.take();
        if !s.dirty {
            drop(s);
            self.send_wheel(wheel, wheel_at);
            return;
        }
        s.dirty = false;
        if let Some((w, h, bytes)) = s.frame.take()
            && let Some(old) = self.image.replace(make_image(w, h, bytes))
        {
            self.stale.push(old);
        }
        if let Some((w, h, bytes)) = s.popup_frame.take()
            && let Some(old) = self.popup_image.replace(make_image(w, h, bytes))
        {
            self.stale.push(old);
        }
        if s.popup_rect.is_none()
            && let Some(old) = self.popup_image.take()
        {
            self.stale.push(old);
        }
        drop(s);
        self.send_wheel(wheel, wheel_at);
        cx.notify();
        if std::env::var_os("COMPOSER_CEF_TRACE").is_some() {
            eprintln!(
                "[cef] after_pump {:.1}ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    /// One wheel call with the delta gathered since the last pump.
    fn send_wheel(&mut self, wheel: (i32, i32), at: Option<(Point<Pixels>, Modifiers)>) {
        let Some((position, modifiers)) = at else {
            return;
        };
        if wheel == (0, 0) {
            return;
        }
        if let Some(host) = self.host() {
            host.send_mouse_wheel_event(Some(&self.mouse(position, &modifiers)), wheel.0, wheel.1);
            // CEF's next frame is requested now: don't wait for the pump's fallback.
            browser_pump::schedule(0);
        }
    }

    fn mouse(&self, pos: Point<Pixels>, mods: &Modifiers) -> MouseEvent {
        let local = pos - self.origin;
        MouseEvent {
            x: f32::from(local.x) as i32,
            y: f32::from(local.y) as i32,
            modifiers: cef_modifiers(mods) | self.buttons_down,
        }
    }

    /// Tells CEF whether the page may take keyboard input (gpui's focus alone is not enough).
    pub fn browser_focus(&mut self, on: bool) {
        if let Some(host) = self.host() {
            host.set_focus(on as _);
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some(host) = self.host() {
            host.set_focus(1);
            let (btn, flag) = button(ev.button);
            self.buttons_down |= flag;
            host.send_mouse_click_event(
                Some(&self.mouse(ev.position, &ev.modifiers)),
                btn,
                0,
                ev.click_count as i32,
            );
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, ev: &MouseUpEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        if let Some(host) = self.host() {
            let (btn, flag) = button(ev.button);
            self.buttons_down &= !flag;
            host.send_mouse_click_event(
                Some(&self.mouse(ev.position, &ev.modifiers)),
                btn,
                1,
                ev.click_count as i32,
            );
        }
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        if let Some(host) = self.host() {
            host.send_mouse_move_event(Some(&self.mouse(ev.position, &ev.modifiers)), 0);
        }
    }

    fn on_scroll(&mut self, ev: &ScrollEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        // CEF's wheel delta is in pixels (a line is ~40px).
        let (dx, dy) = match ev.delta {
            ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(l) => (l.x * 40.0, l.y * 40.0),
        };
        {
            let mut s = self.shared.borrow_mut();
            s.wheel.0 = s.wheel.0.saturating_add(dx as i32);
            s.wheel.1 = s.wheel.1.saturating_add(dy as i32);
            s.wheel_at = Some((ev.position, ev.modifiers));
        }
        // Wake CEF's pump: the accumulated delta goes out in `after_pump`.
        browser_pump::schedule(0);
    }

    /// Non-text keys (and anything with ctrl/alt) go as raw key events; text goes through the IME
    /// path (`replace_text_in_range`), so a printable key isn't delivered twice.
    fn on_key_down(&mut self, ev: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let Some((code, native)) = codes(&ks.key) else {
            return;
        };
        let mods = cef_modifiers(&ks.modifiers);
        if let Some(host) = self.host() {
            host.send_key_event(Some(&key_event(
                sys::cef_key_event_type_t::KEYEVENT_RAWKEYDOWN,
                code,
                native,
                mods,
                0,
            )));
            let textual = ks.key_char.is_some() && !ks.modifiers.control && !ks.modifiers.alt;
            // Keys whose effect comes from a char event in Chromium (enter, tab) need one.
            let ch = match ks.key.as_str() {
                "enter" => Some(13u16),
                "tab" => Some(9u16),
                "backspace" => Some(8u16),
                _ if ks.modifiers.control || ks.modifiers.alt => ks
                    .key
                    .chars()
                    .next()
                    .filter(|_| ks.key.len() == 1)
                    .map(|c| c as u16),
                _ => None,
            };
            if let (Some(ch), false) = (ch, textual) {
                host.send_key_event(Some(&key_event(
                    sys::cef_key_event_type_t::KEYEVENT_CHAR,
                    code,
                    native,
                    mods,
                    ch,
                )));
            }
        }
        cx.notify();
    }

    fn on_key_up(&mut self, ev: &KeyUpEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if let (Some((code, native)), Some(host)) = (codes(&ks.key), self.host()) {
            host.send_key_event(Some(&key_event(
                sys::cef_key_event_type_t::KEYEVENT_KEYUP,
                code,
                native,
                cef_modifiers(&ks.modifiers),
                0,
            )));
        }
    }
}

fn make_image(w: u32, h: u32, bytes: Vec<u8>) -> Arc<RenderImage> {
    // gpui's RenderImage is BGRA, which is what CEF paints: no conversion.
    let buf = image::RgbaImage::from_raw(w, h, bytes).expect("frame size");
    Arc::new(RenderImage::new(SmallVec::from_elem(
        image::Frame::new(buf),
        1,
    )))
}

fn button(b: MouseButton) -> (MouseButtonType, u32) {
    use sys::cef_mouse_button_type_t::*;
    match b {
        MouseButton::Right => (MBT_RIGHT.into(), 1 << 6),
        MouseButton::Middle => (MBT_MIDDLE.into(), 1 << 5),
        _ => (MBT_LEFT.into(), 1 << 4),
    }
}

fn cef_modifiers(m: &Modifiers) -> u32 {
    // EVENTFLAG_SHIFT_DOWN=2, CONTROL=4, ALT=8, COMMAND=128
    (m.shift as u32) << 1 | (m.control as u32) << 2 | (m.alt as u32) << 3 | (m.platform as u32) << 7
}

fn key_event(
    kind: sys::cef_key_event_type_t,
    code: i32,
    native: i32,
    modifiers: u32,
    ch: u16,
) -> KeyEvent {
    KeyEvent {
        size: std::mem::size_of::<sys::_cef_key_event_t>(),
        type_: kind.into(),
        modifiers,
        windows_key_code: code,
        native_key_code: native,
        is_system_key: 0,
        character: ch,
        unmodified_character: ch,
        focus_on_editable_field: 0,
    }
}

impl Focusable for WebPane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for WebPane {
    fn text_for_range(
        &mut self,
        _r: Range<usize>,
        _a: &mut Option<Range<usize>>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }
    fn selected_text_range(
        &mut self,
        _i: bool,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _w: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|m| 0..m.encode_utf16().count())
    }
    fn unmark_text(&mut self, _w: &mut Window, _cx: &mut Context<Self>) {
        if self.marked.take().is_some()
            && let Some(host) = self.host()
        {
            host.ime_finish_composing_text(0);
        }
    }
    fn replace_text_in_range(
        &mut self,
        _r: Option<Range<usize>>,
        text: &str,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.host() {
            if self.marked.take().is_some() {
                host.ime_commit_text(Some(&text.into()), None, 0);
            } else {
                for ch in text.encode_utf16() {
                    host.send_key_event(Some(&key_event(
                        sys::cef_key_event_type_t::KEYEVENT_CHAR,
                        ch as i32,
                        0,
                        0,
                        ch,
                    )));
                }
            }
        }
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _r: Option<Range<usize>>,
        text: &str,
        _sel: Option<Range<usize>>,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.host() {
            let len = text.encode_utf16().count() as u32;
            let sel = cef::Range { from: len, to: len };
            host.ime_set_composition(Some(&text.into()), None, None, Some(&sel));
        }
        self.marked = Some(text.to_string());
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _r: Range<usize>,
        bounds: Bounds<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        _p: Point<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for WebPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity: Entity<Self> = cx.entity();
        let image = self.image.clone();
        let popup = self
            .popup_image
            .clone()
            .zip(self.shared.borrow().popup_rect.clone());
        let stale = std::mem::take(&mut self.stale);
        let shared = self.shared.clone();
        let host = self.host();
        let focus = self.focus.clone();
        let cursor = self.shared.borrow().cursor;
        div()
            .size_full()
            .track_focus(&self.focus)
            .cursor(cursor)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll(cx.listener(Self::on_scroll))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .child(
                canvas(
                    move |bounds, _w, _cx| bounds,
                    move |bounds, _, window, cx| {
                        for old in stale {
                            let _ = window.drop_image(old);
                        }
                        // Keep CEF's view rect in step with the pane.
                        let size = (
                            f32::from(bounds.size.width) as i32,
                            f32::from(bounds.size.height) as i32,
                        );
                        let scale = window.scale_factor();
                        // CEF calls back into view_rect/screen_info synchronously from these host
                        // calls, so the borrow must be released first.
                        let changed = {
                            let mut s = shared.borrow_mut();
                            let changed = s.size != size || s.scale != scale;
                            s.size = size;
                            s.scale = scale;
                            changed
                        };
                        if changed && let Some(host) = &host {
                            host.notify_screen_info_changed();
                            host.was_resized();
                        }
                        entity.update(cx, |pane, _| pane.origin = bounds.origin);
                        if let Some(img) = image {
                            let _ = window.paint_image(
                                bounds,
                                bounds,
                                Default::default(),
                                img,
                                0,
                                false,
                            );
                        }
                        if let Some((img, r)) = popup {
                            let pb = Bounds::new(
                                bounds.origin + point(px(r.x as f32), px(r.y as f32)),
                                gpui::size(px(r.width as f32), px(r.height as f32)),
                            );
                            let _ = window.paint_image(pb, pb, Default::default(), img, 0, false);
                        }
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                    },
                )
                .size_full(),
            )
    }
}

impl Drop for WebPane {
    fn drop(&mut self) {
        if let Some(host) = self.host() {
            host.close_browser(1);
        }
    }
}
