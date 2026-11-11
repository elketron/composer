//! A browser pane: CEF renders offscreen and the pane draws its frames; mouse, wheel, keys and IME
//! go back through the browser host. Links to the workspace's pages open as tabs (events), popups
//! never get windows. Main thread only: CEF runs on the external pump (`browser_pump`).
//! Rule from the spike: never call into the browser host while holding `Shared`'s borrow (CEF
//! calls back into the handlers synchronously).

use std::{
    cell::RefCell,
    ops::Range,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::browser_frame::BrowserFrame;
use crate::browser_pump;
use crate::dom_keys::{Held, codes, modifier_changes};
use crate::view_urls::{AuthCookie, Navigation, navigation, page_for};
use cef::{rc::Rc as _, *};
use gpui::{
    App, Bounds, Context, CursorStyle, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, KeyDownEvent, KeyUpEvent, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, ScrollDelta, ScrollEvent,
    UTF16Selection, Window, canvas, div, point, prelude::*, px,
};

/// What CEF's callbacks write and the pane reads. Main thread only.
#[derive(Default)]
pub struct Shared {
    /// False for background tabs / hidden windows. Late CEF paints are ignored too.
    pub visible: bool,
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
    /// The page answered 401 or 404: the pane's session may be gone (expired, or the server
    /// restarted).
    pub session_lost: bool,
}

#[derive(Clone)]
struct Handler {
    shared: Rc<RefCell<Shared>>,
    /// The workspace base, for telling page links from the rest.
    base: Rc<str>,
    /// The URL this pane was made to show (ours, so it always loads).
    start: Rc<str>,
}

impl Handler {
    /// Whether to cancel a navigation of this pane. Every main-frame and subframe navigation
    /// goes through here, whatever started it (a click, script, meta refresh, a redirect).
    fn cancels(&self, url: &str, main: bool, gesture: bool) -> bool {
        if main
            && gesture
            && let Some(page) = page_for(url, &self.base)
        {
            self.shared.borrow_mut().open_pages.push(page);
            return true;
        }
        match navigation(url, &self.base, &self.start, main, gesture) {
            Navigation::Allow => false,
            Navigation::Block { outside } => {
                if outside {
                    self.shared.borrow_mut().open_outside.push(url.to_string());
                } else {
                    crate::trace(&format!(
                        "pane cancelled {} navigation to {}",
                        if main { "main-frame" } else { "subframe" },
                        url.split(['?', '#']).next().unwrap_or("")
                    ));
                }
                true
            }
        }
    }
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
            if !self.handler.shared.borrow().visible {
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
        // Every navigation, also the first, redirects and subframes (CEF: "Called on the UI
        // thread before browser navigation", with `is_redirect`; include/cef_request_handler.h).
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let Some(request) = request else { return 1 };
            let main = frame.is_none_or(|f| f.is_main() != 0);
            let url = CefStringUtf16::from(&request.url()).to_string();
            self.handler.cancels(&url, main, user_gesture != 0) as _
        }

        // Ctrl/middle click and top-level file: navigations arrive here first.
        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let url = target_url.map(|u| u.to_string()).unwrap_or_default();
            self.handler.cancels(&url, true, user_gesture != 0) as _
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            Some(NoOsProtocolsBuilder::new())
        }

        // False: the request is cancelled (no certificate exception, no way to bypass).
        fn on_certificate_error(
            &self,
            _browser: Option<&mut Browser>,
            _cert_error: Errorcode,
            _request_url: Option<&CefString>,
            _ssl_info: Option<&mut Sslinfo>,
            _callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            0
        }

        // No client certificate, and no chooser dialog (there is no UI for one).
        fn on_select_client_certificate(
            &self,
            _browser: Option<&mut Browser>,
            _is_proxy: ::std::os::raw::c_int,
            _host: Option<&CefString>,
            _port: ::std::os::raw::c_int,
            _certificates: Option<&[Option<X509Certificate>]>,
            callback: Option<&mut SelectClientCertificateCallback>,
        ) -> ::std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.select(None);
            }
            1
        }

        // False: no credentials, the request is cancelled.
        fn auth_credentials(
            &self,
            _browser: Option<&mut Browser>,
            _origin_url: Option<&CefString>,
            _is_proxy: ::std::os::raw::c_int,
            _host: Option<&CefString>,
            _port: ::std::os::raw::c_int,
            _realm: Option<&CefString>,
            _scheme: Option<&CefString>,
            _callback: Option<&mut AuthCallback>,
        ) -> ::std::os::raw::c_int {
            0
        }
    }
}

wrap_resource_request_handler! {
    struct NoOsProtocolsBuilder;

    impl ResourceRequestHandler {
        // Every request goes ahead (the navigation policy decided in `on_before_browse`). This
        // must be stated: the binding's default is `RV_CANCEL`, which cancels every request
        // of the pane and leaves it blank.
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::from(sys::cef_return_value_t::RV_CONTINUE)
        }

        // A URL with a protocol the browser doesn't know never goes to the OS's handler.
        fn on_protocol_execution(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            allow_os_execution: Option<&mut ::std::os::raw::c_int>,
        ) {
            if let Some(allow) = allow_os_execution {
                *allow = 0;
            }
        }
    }
}

wrap_permission_handler! {
    struct PermissionHandlerBuilder;

    impl PermissionHandler {
        // Camera, microphone, screen capture: refused. (Chrome style would otherwise show a
        // prompt in a UI an offscreen pane doesn't have.)
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> ::std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cancel();
            }
            1
        }

        // Notifications, geolocation, clipboard reads, ...: denied.
        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            _prompt_id: u64,
            _requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> ::std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cont(PermissionRequestResult::from(
                    sys::cef_permission_request_result_t::CEF_PERMISSION_RESULT_DENY,
                ));
            }
            1
        }
    }
}

wrap_download_handler! {
    struct DownloadHandlerBuilder;

    impl DownloadHandler {
        // A pane never downloads: refused before it starts, again when it would be named, and
        // cancelled if it still reports progress.
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> ::std::os::raw::c_int {
            0
        }

        // True without running the callback: the download is cancelled (include/cef_download_handler.h).
        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            _download_item: Option<&mut DownloadItem>,
            _suggested_name: Option<&CefString>,
            _callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            _download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            if let Some(callback) = callback {
                callback.cancel();
            }
        }
    }
}

wrap_jsdialog_handler! {
    struct JsdialogHandlerBuilder;

    impl JsdialogHandler {
        // alert/confirm/prompt: suppressed (dismissed without a dialog; there is no UI).
        fn on_jsdialog(
            &self,
            _browser: Option<&mut Browser>,
            _origin_url: Option<&CefString>,
            _dialog_type: JsdialogType,
            _message_text: Option<&CefString>,
            _default_prompt_text: Option<&CefString>,
            _callback: Option<&mut JsdialogCallback>,
            suppress_message: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let Some(suppress) = suppress_message {
                *suppress = 1;
            }
            0
        }

        // "Leave this page?": yes, at once.
        fn on_before_unload_dialog(
            &self,
            _browser: Option<&mut Browser>,
            _message_text: Option<&CefString>,
            _is_reload: ::std::os::raw::c_int,
            callback: Option<&mut JsdialogCallback>,
        ) -> ::std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cont(1, None);
            }
            1
        }
    }
}

wrap_load_handler! {
    struct LoadHandlerBuilder {
        handler: Handler,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: ::std::os::raw::c_int,
        ) {
            let main = frame.is_some_and(|f| f.is_main() != 0);
            if main {
                crate::trace(&format!("pane: page loaded, status {http_status_code}"));
            }
            if main && matches!(http_status_code, 401 | 404) {
                self.handler.shared.borrow_mut().session_lost = true;
            }
        }
    }
}

wrap_set_cookie_callback! {
    struct StartAfterCookie {
        stored: Arc<AtomicBool>,
    }

    impl SetCookieCallback {
        // The cookie is in the store. The pane loads its page from its next pump
        // (`WebPane::load_when_ready`), not from here: this may run before the browser has
        // committed its first page, and a load issued then is lost.
        fn on_complete(&self, success: ::std::os::raw::c_int) {
            if success == 0 {
                crate::trace("pane: the auth cookie was not stored");
            } else {
                crate::trace("pane: the auth cookie is stored");
            }
            self.stored.store(true, Ordering::Release);
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
            // No windows of their own: a page opens as a tab, a link as the user's browser,
            // anything else is dropped.
            let url = target_url.map(|u| u.to_string()).unwrap_or_default();
            self.handler.cancels(&url, true, true);
            1
        }
    }
}

/// The browser's client: one handler of each kind (its generated `new` takes them all).
#[allow(
    clippy::too_many_arguments,
    reason = "the macro's constructor takes one argument per handler"
)]
mod client {
    use super::*;

    wrap_client! {
        pub(super) struct ClientBuilder {
            render: RenderHandler,
            display: DisplayHandler,
            request: RequestHandler,
            life_span: LifeSpanHandler,
            permission: PermissionHandler,
            download: DownloadHandler,
            jsdialog: JsdialogHandler,
            load: LoadHandler,
        }

        impl Client {
            fn load_handler(&self) -> Option<cef::LoadHandler> {
                Some(self.load.clone())
            }
            fn permission_handler(&self) -> Option<cef::PermissionHandler> {
                Some(self.permission.clone())
            }
            fn download_handler(&self) -> Option<cef::DownloadHandler> {
                Some(self.download.clone())
            }
            fn jsdialog_handler(&self) -> Option<cef::JsdialogHandler> {
                Some(self.jsdialog.clone())
            }
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
}
use client::ClientBuilder;

/// Puts the auth cookie in the global request context (the panes' profile). The returned flag
/// is set once the cookie is stored (or could not be: the page then loads without it, the
/// server answers 404 and the tab says so).
fn store_cookie(cookie: &AuthCookie) -> Arc<AtomicBool> {
    let stored = Arc::new(AtomicBool::new(false));
    let manager = request_context_get_global_context().and_then(|c| c.cookie_manager(None));
    let entry = Cookie {
        name: cookie.name.as_str().into(),
        value: cookie.value.as_str().into(),
        path: cookie.path.as_str().into(),
        secure: cookie.secure as _,
        httponly: 1,
        same_site: CookieSameSite::STRICT_MODE,
        ..Default::default()
    };
    let mut done = StartAfterCookie::new(stored.clone());
    let accepted = manager.is_some_and(|m| {
        m.set_cookie(
            Some(&cookie.url.as_str().into()),
            Some(&entry),
            Some(&mut done),
        ) != 0
    });
    if !accepted {
        done.on_complete(0);
    }
    stored
}

pub struct WebPane {
    pub shared: Rc<RefCell<Shared>>,
    browser: Option<Browser>,
    /// The browser's handlers, kept to make the browser once its cookie is stored.
    client: Client,
    /// The URL it was made to show.
    url: String,
    /// It authenticates with a session cookie that it may renew once (see
    /// [`WebEvent::SessionLost`]).
    renewable: bool,
    /// Its page, waiting for its cookie to be stored (see [`Self::load_when_ready`]).
    pending: Option<Arc<AtomicBool>>,
    focus: FocusHandle,
    origin: Point<Pixels>,
    image: BrowserFrame,
    popup_image: BrowserFrame,
    marked: Option<String>,
    buttons_down: u32,
    /// The page is the live view in Drive mode's reach (see [`Self::set_drive`]).
    drive: bool,
    /// Modifier keys as last passed to the page (drive panes only).
    held: Held,
}

/// What a browser pane asks of its window.
pub enum WebEvent {
    /// A link to a page: open it as a tab.
    OpenPage(String),
    /// A link outside the workspace: the OS opens it.
    OpenOutside(String),
    /// A pane that authenticates with a session cookie got 401/404: its session may be gone.
    /// The window gets a new one and calls [`WebPane::renew`] (once per pane).
    SessionLost,
}

impl EventEmitter<WebEvent> for WebPane {}

impl WebPane {
    /// A browser showing `url`; links to pages of the workspace at `base` become [`WebEvent`]s.
    /// With `cookie`, it is stored in CEF first and the browser is made once it is there (so
    /// the token never appears in a URL). It is made directly on `url`: a browser started on
    /// `about:blank` and sent on with `load_url` never left `about:blank` (CEF 156,
    /// windowless).
    pub fn new(url: &str, base: &str, cookie: Option<AuthCookie>, cx: &mut Context<Self>) -> Self {
        let shared = Rc::new(RefCell::new(Shared {
            size: (800, 600),
            scale: 1.0,
            ..Default::default()
        }));
        let handler = Handler {
            shared: shared.clone(),
            base: base.into(),
            start: url.into(),
        };
        let client = ClientBuilder::new(
            RenderHandlerBuilder::new(handler.clone()),
            DisplayHandlerBuilder::new(handler.clone()),
            RequestHandlerBuilder::new(handler.clone()),
            LifeSpanHandlerBuilder::new(handler.clone()),
            PermissionHandlerBuilder::new(),
            DownloadHandlerBuilder::new(),
            JsdialogHandlerBuilder::new(),
            LoadHandlerBuilder::new(handler),
        );
        let renewable = cookie.is_some();
        let pending = cookie.as_ref().map(store_cookie);
        browser_pump::register(cx.weak_entity());
        let mut pane = Self {
            shared,
            browser: None,
            client,
            url: url.to_string(),
            renewable,
            pending,
            focus: cx.focus_handle(),
            origin: point(px(0.), px(0.)),
            image: BrowserFrame::default(),
            popup_image: BrowserFrame::default(),
            marked: None,
            buttons_down: 0,
            drive: false,
            held: Held::default(),
        };
        if pane.pending.is_none() {
            pane.create_browser();
        }
        pane
    }

    /// Makes the browser, on the pane's page.
    fn create_browser(&mut self) {
        let window_info = WindowInfo {
            windowless_rendering_enabled: 1,
            ..Default::default()
        };
        let settings = BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };
        self.browser = browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut self.client),
            Some(&self.url.as_str().into()),
            Some(&settings),
            None,
            None,
        );
        // Hidden until the shell shows the tab (which may already be in the background).
        let visible = self.shared.borrow().visible;
        if let Some(host) = self.host() {
            host.was_hidden((!visible) as _);
        }
    }

    /// Stores `cookie` (a new session) and loads the pane's page again.
    pub fn renew(&mut self, cookie: AuthCookie) {
        self.pending = Some(store_cookie(&cookie));
    }

    /// Once the pane's cookie is stored: makes the browser on its page, or (after `renew`)
    /// loads the page again in the browser it has.
    fn load_when_ready(&mut self) {
        let ready = self
            .pending
            .as_ref()
            .is_some_and(|stored| stored.load(Ordering::Acquire));
        if !ready {
            return;
        }
        self.pending = None;
        match self.browser.as_ref().and_then(|b| b.main_frame()) {
            Some(frame) => {
                crate::trace("pane: loading its page again with a new session");
                frame.load_url(Some(&self.url.as_str().into()));
            }
            None => {
                crate::trace("pane: making its browser");
                self.create_browser();
            }
        }
    }

    /// Makes this pane pass on what the live view's Drive mode needs and an ordinary page does
    /// not care about, so other tabs behave as before:
    ///
    /// - modifier keys on their own (gpui reports them only as a change of state), so shift, ctrl
    ///   and alt held while clicking or dragging reach the application;
    /// - auto-repeat marked as such (the page ignores repeats; the application repeats a held
    ///   key itself);
    /// - a drag that leaves the pane: moves and the button release outside it still reach the
    ///   page, which captured the pointer on the press (orbiting a 3D view).
    pub fn set_drive(&mut self, on: bool) {
        self.drive = on;
    }

    fn host(&self) -> Option<BrowserHost> {
        self.browser.as_ref().and_then(|b| b.host())
    }

    /// Visibility is independent of keyboard focus: an unfocused but visible pane still paints.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        {
            let mut shared = self.shared.borrow_mut();
            if shared.visible == visible {
                return;
            }
            shared.visible = visible;
            if !visible {
                shared.frame = None;
                shared.popup_frame = None;
                shared.popup_rect = None;
                shared.wheel = (0, 0);
                shared.wheel_at = None;
            }
        }
        if !visible {
            self.image.discard_pending();
            self.popup_image.clear();
        }
        // CEF can call the handlers synchronously: Shared must not be borrowed here.
        if let Some(host) = self.host() {
            if !visible {
                host.set_focus(0);
            }
            host.was_hidden((!visible) as _);
        }
        if visible {
            cx.notify();
        }
    }

    /// After CEF's work: retain only the latest pixels; links to pages become events.
    /// RenderImages are created only when the canvas actually paints.
    pub fn after_pump(&mut self, cx: &mut Context<Self>) {
        let started = std::time::Instant::now();
        self.load_when_ready();
        let (pages, outside, lost) = {
            let mut s = self.shared.borrow_mut();
            (
                std::mem::take(&mut s.open_pages),
                std::mem::take(&mut s.open_outside),
                std::mem::take(&mut s.session_lost),
            )
        };
        if lost && std::mem::take(&mut self.renewable) {
            cx.emit(WebEvent::SessionLost);
        }
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
        if let Some((w, h, bytes)) = s.frame.take() {
            self.image.replace(w, h, bytes);
        }
        if let Some((w, h, bytes)) = s.popup_frame.take() {
            self.popup_image.replace(w, h, bytes);
        }
        if s.popup_rect.is_none() {
            self.popup_image.clear();
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

    /// Modifier keys pressed or released by themselves (drive panes, while focused).
    fn on_modifiers(&mut self, modifiers: Modifiers) {
        let now = Held {
            shift: modifiers.shift,
            control: modifiers.control,
            alt: modifiers.alt,
            platform: modifiers.platform,
        };
        let changes = modifier_changes(self.held, now);
        self.held = now;
        let Some(host) = self.host() else {
            return;
        };
        let mods = cef_modifiers(&modifiers);
        for (down, code, native) in changes {
            let kind = if down {
                sys::cef_key_event_type_t::KEYEVENT_RAWKEYDOWN
            } else {
                sys::cef_key_event_type_t::KEYEVENT_KEYUP
            };
            host.send_key_event(Some(&key_event(kind, code, native, mods, 0)));
        }
    }

    /// A drag with a button held that is outside the pane (drive panes): the page still gets the
    /// motion, and the release that would otherwise be lost.
    fn on_drag_outside(
        &mut self,
        position: Point<Pixels>,
        modifiers: &Modifiers,
        release: Option<MouseButton>,
    ) {
        let Some(host) = self.host() else {
            return;
        };
        let event = self.mouse(position, modifiers);
        match release {
            None => host.send_mouse_move_event(Some(&event), 0),
            Some(b) => {
                let (btn, flag) = button(b);
                self.buttons_down &= !flag;
                host.send_mouse_click_event(Some(&event), btn, 1, 1);
            }
        }
    }

    /// Non-text keys (and anything with ctrl/alt) go as raw key events; text goes through the IME
    /// path (`replace_text_in_range`), so a printable key isn't delivered twice.
    fn on_key_down(&mut self, ev: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let Some((code, native)) = codes(&ks.key) else {
            return;
        };
        let mut mods = cef_modifiers(&ks.modifiers);
        if self.drive && ev.is_held {
            mods |= EVENTFLAG_IS_REPEAT;
        }
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

/// CEF's `EVENTFLAG_IS_REPEAT`.
const EVENTFLAG_IS_REPEAT: u32 = 1 << 13;

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
        let shared = self.shared.clone();
        let host = self.host();
        let focus = self.focus.clone();
        let cursor = self.shared.borrow().cursor;
        let drive = self.drive;
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
            .on_modifiers_changed(cx.listener(|pane, ev: &gpui::ModifiersChangedEvent, _, _| {
                if pane.drive {
                    pane.on_modifiers(ev.modifiers);
                }
            }))
            .child(
                canvas(
                    move |bounds, _w, _cx| bounds,
                    move |bounds, _, window, cx| {
                        // Materialize only what is actually about to be drawn. after_pump
                        // merely replaces pending pixels, even if the window isn't painting.
                        let (image, popup) = entity.update(cx, |pane, _| {
                            let image = pane.image.present(|old| {
                                let _ = window.drop_image(old);
                            });
                            let popup = pane
                                .popup_image
                                .present(|old| {
                                    let _ = window.drop_image(old);
                                })
                                .zip(pane.shared.borrow().popup_rect.clone());
                            (image, popup)
                        });
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
                        if drive {
                            listen_for_drive(&entity, bounds, window);
                        }
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

/// What a drive pane listens to beyond its own element: the pointer once a button is down and it
/// has left `bounds`. (Modifier keys come to the pane's own element, which has the focus.)
fn listen_for_drive(entity: &Entity<WebPane>, bounds: Bounds<Pixels>, window: &mut Window) {
    let pane = entity.clone();
    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
        let outside = !bounds.contains(&ev.position);
        if phase == gpui::DispatchPhase::Capture && outside {
            pane.update(cx, |pane, _| {
                if pane.buttons_down != 0 {
                    pane.on_drag_outside(ev.position, &ev.modifiers, None);
                }
            });
        }
    });
    let pane = entity.clone();
    window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
        let outside = !bounds.contains(&ev.position);
        if phase == gpui::DispatchPhase::Capture && outside {
            pane.update(cx, |pane, _| {
                if pane.buttons_down != 0 {
                    pane.on_drag_outside(ev.position, &ev.modifiers, Some(ev.button));
                }
            });
        }
    });
}

impl Drop for WebPane {
    fn drop(&mut self) {
        if let Some(host) = self.host() {
            host.close_browser(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_paints_for_hidden_tabs_are_ignored_and_visible_paints_still_arrive() {
        let shared = Rc::new(RefCell::new(Shared::default()));
        let handler = RenderHandlerBuilder::new(Handler {
            shared: shared.clone(),
            base: "http://127.0.0.1:1".into(),
            start: "about:blank".into(),
        });
        let pixels = [7_u8; 16];
        let view = PaintElementType::from(sys::cef_paint_element_type_t::PET_VIEW);
        let popup = PaintElementType::from(sys::cef_paint_element_type_t::PET_POPUP);
        handler.on_paint(None, view, None, pixels.as_ptr(), 2, 2);
        handler.on_paint(None, popup, None, pixels.as_ptr(), 2, 2);
        assert!(shared.borrow().frame.is_none());
        assert!(shared.borrow().popup_frame.is_none());
        assert!(!shared.borrow().dirty);

        shared.borrow_mut().visible = true;
        handler.on_paint(None, view, None, pixels.as_ptr(), 2, 2);
        handler.on_paint(None, popup, None, pixels.as_ptr(), 2, 2);
        let shared = shared.borrow();
        assert_eq!(shared.frame.as_ref().unwrap().2, pixels);
        assert_eq!(shared.popup_frame.as_ref().unwrap().2, pixels);
        assert!(shared.dirty);
    }
}
