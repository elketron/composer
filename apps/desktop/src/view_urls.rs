//! Where a stage tab loads from, and which navigations are really "open this page as a tab".

use composer_client::Endpoint;
use composer_protocol::DeskTab;
use composer_protocol::http::WebSession;

/// `http://host:port` of a server base like `http://host:port/some/path`.
pub fn origin(base: &str) -> &str {
    let after_scheme = base.find("://").map_or(0, |i| i + 3);
    match base[after_scheme..].find('/') {
        Some(slash) => &base[..after_scheme + slash],
        None => base,
    }
}

/// The URL a page, changes or live view tab shows. It never carries the token: the pane
/// authenticates with a browser session the server made for it, set as the server's cookie
/// through CEF before the first load ([`auth_cookie`]), so neither the token nor the session is in
/// `document.URL` or the profile's history. The live view is wayai's full page through the server's proxy
/// (`<base>/view`, not `?bare=1`): its toolbar, annotations and Drive. Whether Drive is offered
/// is the server's call, by the credential: a paired server's device token gets the page
/// without it.
pub fn view_url(endpoint: &Endpoint, tab: &DeskTab) -> Option<String> {
    let path = match tab {
        DeskTab::Page { url } => format!("/view/{}", url.strip_prefix("site://")?),
        DeskTab::Changes { site } => format!("/view/changes/{}", encode(site)),
        DeskTab::LiveView => "/view".to_string(),
        // Through composer's proxy, so the same address works wherever composer does.
        DeskTab::App { name, path } if valid_app_name(name) => {
            format!("/apps/{name}/{}", path.trim_start_matches('/'))
        }
        _ => return None,
    };
    Some(endpoint.url(&path))
}

/// The server's auth cookie for the panes: a browser session the server made for this
/// desktop's credential (`POST <base>/api/web-session`, [`WebSession`]), never the token itself.
/// Host-only, `Path=<route base>/`, HttpOnly, SameSite=Strict, `Secure` over https; a session
/// cookie, as the server's own `set_cookie` (`crates/server/src/api/auth.rs`) sets it.
#[derive(Debug, PartialEq, Eq)]
pub struct AuthCookie {
    /// Any URL under the cookie's path (CEF derives the host from it).
    pub url: String,
    pub name: String,
    pub value: String,
    pub path: String,
    pub secure: bool,
}

/// The cookie that authenticates panes of the server at `endpoint` with `session`; `None` for
/// a base that is not http(s).
pub fn auth_cookie(endpoint: &Endpoint, session: &WebSession) -> Option<AuthCookie> {
    let parts = parts(endpoint.base())?;
    let path = format!("/{}", session.path.trim_matches('/'));
    let path = format!("{}/", path.trim_end_matches('/'));
    Some(AuthCookie {
        url: format!("{}{path}", origin(endpoint.base())),
        name: session.cookie_name.clone(),
        value: session.value.clone(),
        path,
        secure: parts.scheme == "https",
    })
}

/// A URL split for comparing origins.
#[derive(Debug, PartialEq, Eq)]
struct Parts {
    scheme: &'static str,
    host: String,
    port: u16,
    userinfo: bool,
}

/// `http(s)://[userinfo@]host[:port]...`: scheme and host lowercased, the default port filled
/// in, an IPv6 literal normalised. Anything else (other schemes, no authority, control or
/// space characters, backslashes or `%` in the authority, a bad port or host) is `None`. A
/// trailing dot on the host is kept: `host.` is another origin, as in a browser.
fn parts(url: &str) -> Option<Parts> {
    if url.chars().any(|c| c.is_control() || c == ' ') {
        return None;
    }
    let (scheme, rest) = url.split_once("://")?;
    let (scheme, default) = match scheme.to_ascii_lowercase().as_str() {
        "http" => ("http", 80),
        "https" => ("https", 443),
        _ => return None,
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains(['\\', '%']) {
        return None;
    }
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(at) => (true, &authority[at + 1..]),
        None => (false, authority),
    };
    let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
        let (inner, after) = v6.split_once(']')?;
        let addr: std::net::Ipv6Addr = inner.parse().ok()?;
        let port = match after {
            "" => "",
            _ => after.strip_prefix(':')?,
        };
        (format!("[{addr}]"), port)
    } else {
        let (host, port) = hostport.rsplit_once(':').unwrap_or((hostport, ""));
        if host.is_empty() || host.contains(['[', ']', '@', ':']) {
            return None;
        }
        (host.to_ascii_lowercase(), port)
    };
    // `host:` is the default port, as in a browser.
    let port = if port.is_empty() {
        default
    } else if port.bytes().all(|b| b.is_ascii_digit()) {
        port.parse().ok()?
    } else {
        return None;
    };
    Some(Parts {
        scheme,
        host,
        port,
        userinfo,
    })
}

/// Whether two http(s) URLs have the same scheme, host and port (default ports normalised, host
/// case-insensitive, userinfo ignored). Anything unparsable is not the same origin as anything.
pub fn same_origin(a: &str, b: &str) -> bool {
    match (parts(a), parts(b)) {
        (Some(a), Some(b)) => (a.scheme, &a.host, a.port) == (b.scheme, &b.host, b.port),
        _ => false,
    }
}

/// `link` is on the server's origin and carries no userinfo.
fn ours(link: &str, base: &str) -> bool {
    same_origin(link, base) && parts(link).is_some_and(|p| !p.userinfo)
}

/// Whether `name` can be an app's name (as the server's `app_upstream::valid_name`): a path
/// segment of letters, digits, `-` and `_`.
fn valid_app_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// What a pane shows, and so where it may go: composer's own pages, or one app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    /// A page, the changes or the live view: the server's origin, except its apps.
    Pages,
    /// The app of this name: only under `/apps/<name>/`.
    App(&'a str),
}

impl<'a> Scope<'a> {
    pub fn of(tab: &'a DeskTab) -> Self {
        match tab {
            DeskTab::App { name, .. } => Scope::App(name),
            _ => Scope::Pages,
        }
    }
}

/// The app a link on this server is in, and the path in it (as a desk's `App` tab has it,
/// `?query` and `#fragment` kept): `<base>/apps/<name>[/<path>]`. `None` for anything else, a
/// link with userinfo, or a path that tries to climb out (`..`, encoded dots).
pub fn app_for(link: &str, base: &str) -> Option<(String, String)> {
    if !ours(link, base) {
        return None;
    }
    let rest = link.strip_prefix(base)?.strip_prefix("/apps/")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (name, after) = rest.split_at(end);
    if !valid_app_name(name) {
        return None;
    }
    let path = after.strip_prefix('/').unwrap_or(after);
    let route = path.split(['?', '#']).next().unwrap_or("");
    if route.split('/').any(|s| s == "..") || route.to_ascii_lowercase().contains("%2e") {
        return None;
    }
    Some((name.to_string(), path.to_string()))
}

/// What a pane does with a navigation.
#[derive(Debug, PartialEq, Eq)]
pub enum Navigation {
    Allow,
    /// Cancelled; `outside`: the user's own browser opens it instead (a link the user followed).
    Block {
        outside: bool,
    },
}

/// Whether a pane may load `url`, in its main frame or a subframe. Only the server's
/// origin within the pane's [`Scope`] (a page pane: anything but `/apps/`; an app pane: its own
/// `/apps/<name>/`, iframes included), the pane's own start URL (chosen by us) and
/// `about:blank` (plus `about:srcdoc` and same-origin `blob:` in subframes). Whatever the page
/// asks for (script, meta refresh, redirects) goes through this, user gesture or not; only a
/// gesture link to http(s) or mail from the main frame opens outside. (A gesture link to a
/// page or to another app is turned into a tab before this: [`page_for`], [`app_for`].)
pub fn navigation(
    url: &str,
    base: &str,
    start: &str,
    scope: Scope,
    main: bool,
    gesture: bool,
) -> Navigation {
    if url == start || url == "about:blank" || (!main && url == "about:srcdoc") {
        return Navigation::Allow;
    }
    if ours(url, base) {
        let app = app_for(url, base);
        let inside = match scope {
            Scope::Pages => app.is_none() && !in_apps(url, base),
            Scope::App(name) => app.is_some_and(|(n, _)| n == name),
        };
        if inside {
            return Navigation::Allow;
        }
    }
    if let Some(inner) = url.strip_prefix("blob:")
        && ours(inner, base)
    {
        return Navigation::Allow;
    }
    let web = parts(url).is_some();
    let mail = url.len() > 7 && url[..7].eq_ignore_ascii_case("mailto:");
    Navigation::Block {
        outside: main && gesture && (web || mail),
    }
}

/// `link` is under `<base>/apps` at all (a valid app or not).
fn in_apps(link: &str, base: &str) -> bool {
    link.strip_prefix(base)
        .and_then(|rest| rest.strip_prefix("/apps"))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
}

/// The page a clicked link means (`site://...`, or another page's view on this server), so it
/// opens as a tab instead of replacing this one; `None` for anything else.
pub fn page_for(link: &str, base: &str) -> Option<String> {
    if link.starts_with("site://") {
        return Some(link.to_string());
    }
    let rest = link.strip_prefix(base)?.strip_prefix("/view/")?;
    let rest = rest.split(['?', '#']).next()?;
    if rest.is_empty() || rest.starts_with("changes/") {
        return None;
    }
    Some(format!("site://{rest}"))
}

/// Dropped or picked files as text for a terminal: each path quoted for a shell when it needs
/// it, separated by spaces.
pub fn paths_text<'a>(paths: impl Iterator<Item = &'a std::path::Path>) -> String {
    paths
        .map(|p| {
            let p = p.to_string_lossy();
            if p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-+=:@,".contains(c))
            {
                p.into_owned()
            } else {
                format!("'{}'", p.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn encode(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://127.0.0.1:7420";

    #[test]
    fn origins_and_views() {
        assert_eq!(origin("http://127.0.0.1:7420/a/b"), "http://127.0.0.1:7420");
        assert_eq!(origin("http://h:1"), "http://h:1");
        let endpoint = Endpoint::new(BASE, "tok");
        let page = DeskTab::Page {
            url: "site://plans/x.md".into(),
        };
        assert_eq!(
            view_url(&endpoint, &page).unwrap(),
            "http://127.0.0.1:7420/view/plans/x.md"
        );
        let changes = DeskTab::Changes {
            site: "my game".into(),
        };
        assert_eq!(
            view_url(&endpoint, &changes).unwrap(),
            "http://127.0.0.1:7420/view/changes/my%20game"
        );
        assert_eq!(
            view_url(&endpoint, &DeskTab::Terminal { id: "t".into() }),
            None
        );
        // The full viewer page (not `?bare=1`); the cookie authenticates it.
        assert_eq!(
            view_url(&endpoint, &DeskTab::LiveView).unwrap(),
            "http://127.0.0.1:7420/view"
        );
    }

    #[test]
    fn dropped_paths() {
        let paths = [
            std::path::Path::new("/tmp/a.png"),
            std::path::Path::new("/tmp/my file's.txt"),
        ];
        assert_eq!(
            paths_text(paths.into_iter()),
            "/tmp/a.png '/tmp/my file'\\''s.txt'"
        );
    }

    #[test]
    fn links_that_are_pages() {
        assert_eq!(
            page_for("site://plans/x.md", BASE).as_deref(),
            Some("site://plans/x.md")
        );
        assert_eq!(
            page_for(&format!("{BASE}/view/plans/x/notes.md#result"), BASE).as_deref(),
            Some("site://plans/x/notes.md")
        );
        assert_eq!(page_for(&format!("{BASE}/view/changes/game"), BASE), None);
        assert_eq!(
            page_for(&format!("{BASE}/plans/"), BASE),
            None,
            "not a view"
        );
        assert_eq!(page_for("https://example.com/view/x", BASE), None);
    }

    #[test]
    fn no_pane_url_carries_the_token() {
        let endpoint = Endpoint::new(BASE, "sekrit-tok");
        for tab in [
            DeskTab::Page {
                url: "site://plans/x.md".into(),
            },
            DeskTab::Changes { site: "p".into() },
            DeskTab::LiveView,
        ] {
            let url = view_url(&endpoint, &tab).unwrap();
            assert!(!url.contains("token") && !url.contains("sekrit"), "{url}");
        }
        assert_eq!(
            view_url(&endpoint, &DeskTab::Terminal { id: "t".into() }),
            None
        );
    }

    #[test]
    fn the_auth_cookie_is_the_servers_session_never_the_token() {
        let session = |name: &str, path: &str| WebSession {
            cookie_name: name.into(),
            value: "ses_abc".into(),
            path: path.into(),
        };
        let c = auth_cookie(&Endpoint::new(BASE, "tok"), &session("composer_token", "/")).unwrap();
        assert_eq!(c.name, "composer_token");
        assert_eq!(c.value, "ses_abc");
        assert_eq!(c.path, "/");
        assert_eq!(c.url, "http://127.0.0.1:7420/");
        assert!(!c.secure);
        let c = auth_cookie(
            &Endpoint::new("https://h.ts.net", "t"),
            &session("composer_token_443", "/"),
        )
        .unwrap();
        assert_eq!(
            (c.name.as_str(), c.path.as_str(), c.url.as_str()),
            ("composer_token_443", "/", "https://h.ts.net/")
        );
        assert!(c.secure);
        assert_eq!(
            auth_cookie(&Endpoint::new("ftp://h", "t"), &session("c", "/")),
            None
        );
    }

    #[test]
    fn origins_compare_exactly() {
        let b = "http://127.0.0.1:7420";
        assert!(same_origin("http://127.0.0.1:7420/x?y#z", b));
        assert!(same_origin("HTTP://127.0.0.1:7420", b));
        assert!(
            !same_origin("http://127.0.0.1:7420.evil.example/", b),
            "prefix"
        );
        assert!(!same_origin("http://127.0.0.1:7421/", b), "port");
        assert!(!same_origin("https://127.0.0.1:7420/", b), "scheme");
        assert!(!same_origin("http://127.0.0.1.:7420/", b), "trailing dot");
        assert!(
            !same_origin("http://localhost:7420/", b),
            "localhost is not 127.0.0.1"
        );
        assert!(same_origin("http://EXAMPLE.com/", "http://example.com:80/"));
        assert!(same_origin(
            "https://example.com:443/",
            "https://example.com"
        ));
        assert!(
            same_origin("http://example.com:/", "http://example.com"),
            "empty port"
        );
        assert!(!same_origin(
            "http://example.com:443/",
            "http://example.com"
        ));
        // IPv6
        assert!(same_origin(
            "http://[::1]:7420/a",
            "http://[0:0:0:0:0:0:0:1]:7420"
        ));
        assert!(same_origin("http://[::1]/", "http://[::1]:80/"));
        assert!(!same_origin("http://[::1]:7420/", "http://[::2]:7420/"));
        assert!(!same_origin("http://[::1]:7420/", "http://127.0.0.1:7420/"));
        // userinfo is ignored by the comparison, but such a link is not ours
        assert!(same_origin("http://u:p@127.0.0.1:7420/", b));
        assert!(!ours("http://u@127.0.0.1:7420/", b));
        assert!(!ours("http://127.0.0.1:7420@evil.example/", b));
        assert_eq!(
            navigation(
                "http://127.0.0.1:7420@evil.example/",
                b,
                "",
                Scope::Pages,
                true,
                true
            ),
            Navigation::Block { outside: true }
        );
        // malformed is never the same origin
        for bad in [
            "",
            "http://",
            "http:///x",
            "127.0.0.1:7420",
            "http://127.0.0.1:99999/",
            "http://127.0.0.1:ab/",
            "http://[::1/",
            "http://[zz]/",
            "http://a b/",
            "http://127.0.0.1:7420\\@evil/",
            "http://127.0.0.%31:7420/",
            "ftp://127.0.0.1:7420/",
            "javascript:alert(1)",
            "http://\n127.0.0.1:7420/",
            "http://[::1]x:1/",
        ] {
            assert!(!same_origin(bad, b), "{bad:?}");
            assert!(!same_origin(b, bad), "{bad:?}");
        }
    }

    #[test]
    fn what_a_pane_may_load() {
        use Navigation::*;
        let start = "http://127.0.0.1:7420/view";
        let nav = |url, main, gesture| navigation(url, BASE, start, Scope::Pages, main, gesture);
        assert_eq!(nav("http://127.0.0.1:7420/view/a.md", true, false), Allow);
        assert_eq!(nav("http://127.0.0.1:7420/api/sites", true, true), Allow);
        assert_eq!(nav("about:blank", true, false), Allow);
        assert_eq!(nav("about:srcdoc", false, false), Allow);
        assert_eq!(nav("about:srcdoc", true, false), Block { outside: false });
        assert_eq!(nav("blob:http://127.0.0.1:7420/uuid", false, false), Allow);
        assert_eq!(
            nav("blob:https://evil.example/uuid", false, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("data:text/html,x", true, false),
            Block { outside: false }
        );
        assert_eq!(nav(start, true, false), Allow, "its own start URL");
        // a script or redirect to another origin: cancelled silently
        assert_eq!(
            nav("https://evil.example/", true, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420.evil.example/", true, false),
            Block { outside: false }
        );
        // a link the user followed: the user's browser
        assert_eq!(
            nav("https://example.com/", true, true),
            Block { outside: true }
        );
        assert_eq!(nav("mailto:a@b.c", true, true), Block { outside: true });
        assert_eq!(
            nav("file:///etc/passwd", true, true),
            Block { outside: false }
        );
        assert_eq!(
            nav("javascript:alert(1)", true, true),
            Block { outside: false }
        );
        assert_eq!(nav("zoommtg://x", true, true), Block { outside: false });
        // subframes never open outside
        assert_eq!(
            nav("https://example.com/", false, true),
            Block { outside: false }
        );
        assert_eq!(nav("http://127.0.0.1:7420/view", false, false), Allow);
        // a page pane never goes into an app (a followed link becomes the app's tab first)
        assert_eq!(
            nav("http://127.0.0.1:7420/apps/canvas/", true, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420/apps/canvas/x", false, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420/apps", true, false),
            Block { outside: false }
        );
        assert_eq!(nav("http://127.0.0.1:7420/appsx/y", true, false), Allow);
    }

    #[test]
    fn app_tabs_load_through_the_proxy() {
        let endpoint = Endpoint::new(BASE, "sekrit-tok");
        let app = |name: &str, path: &str| DeskTab::App {
            name: name.into(),
            path: path.into(),
        };
        assert_eq!(
            view_url(&endpoint, &app("canvas", "")).unwrap(),
            "http://127.0.0.1:7420/apps/canvas/"
        );
        assert_eq!(
            view_url(&endpoint, &app("canvas", "board/1?zoom=2")).unwrap(),
            "http://127.0.0.1:7420/apps/canvas/board/1?zoom=2"
        );
        assert_eq!(
            view_url(&endpoint, &app("canvas", "/x")).unwrap(),
            "http://127.0.0.1:7420/apps/canvas/x"
        );
        assert_eq!(view_url(&endpoint, &app("../api", "")), None);
        assert_eq!(view_url(&endpoint, &app("", "")), None);
        assert_eq!(Scope::of(&app("canvas", "")), Scope::App("canvas"));
        assert_eq!(Scope::of(&DeskTab::LiveView), Scope::Pages);
    }

    #[test]
    fn links_that_are_apps() {
        let app = |link: &str| app_for(link, BASE);
        let some = |name: &str, path: &str| Some((name.to_string(), path.to_string()));
        assert_eq!(app(&format!("{BASE}/apps/canvas")), some("canvas", ""));
        assert_eq!(app(&format!("{BASE}/apps/canvas/")), some("canvas", ""));
        assert_eq!(
            app(&format!("{BASE}/apps/canvas/api/nodes/7/app?x=1#top")),
            some("canvas", "api/nodes/7/app?x=1#top")
        );
        assert_eq!(
            app(&format!("{BASE}/apps/canvas?x=1")),
            some("canvas", "?x=1")
        );
        assert_eq!(app(&format!("{BASE}/apps/")), None);
        assert_eq!(app(&format!("{BASE}/apps")), None);
        assert_eq!(app(&format!("{BASE}/apps/a%20b/")), None);
        assert_eq!(app(&format!("{BASE}/apps/canvas/../../api")), None);
        assert_eq!(app(&format!("{BASE}/apps/canvas/%2E%2E/api")), None);
        assert_eq!(app(&format!("{BASE}/view/x.md")), None);
        assert_eq!(app("http://127.0.0.1:7421/apps/canvas/"), None, "port");
        assert_eq!(
            app("http://u@127.0.0.1:7420/apps/canvas/"),
            None,
            "userinfo"
        );
        assert_eq!(app("https://example.com/apps/canvas/"), None);
    }

    #[test]
    fn what_an_app_pane_may_load() {
        use Navigation::*;
        let start = "http://127.0.0.1:7420/apps/canvas/";
        let nav =
            |url, main, gesture| navigation(url, BASE, start, Scope::App("canvas"), main, gesture);
        assert_eq!(nav(start, true, false), Allow);
        // its own pages, links and iframes (cards) stay in the pane
        assert_eq!(
            nav("http://127.0.0.1:7420/apps/canvas/board", true, true),
            Allow
        );
        assert_eq!(nav("http://127.0.0.1:7420/apps/canvas", true, false), Allow);
        assert_eq!(
            nav(
                "http://127.0.0.1:7420/apps/canvas/api/nodes/n1/app",
                false,
                false
            ),
            Allow
        );
        assert_eq!(nav("about:srcdoc", false, false), Allow);
        assert_eq!(nav("blob:http://127.0.0.1:7420/uuid", false, false), Allow);
        // composer's own pages and other apps: never loaded in the pane (a followed link to one
        // is turned into its tab before this)
        assert_eq!(
            nav("http://127.0.0.1:7420/view/plans/x.md", true, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420/view", false, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420/api/sites", false, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420/apps/other/", true, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420/apps/canvasx/", false, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("http://127.0.0.1:7420/apps/canvas/../../api", false, false),
            Block { outside: false }
        );
        // the app's own server directly, or anything else: outside when followed, else nothing
        assert_eq!(
            nav("http://127.0.0.1:4477/", true, true),
            Block { outside: true }
        );
        assert_eq!(
            nav("http://127.0.0.1:4477/", false, false),
            Block { outside: false }
        );
        assert_eq!(
            nav("https://example.com/", true, true),
            Block { outside: true }
        );
        assert_eq!(
            nav("https://example.com/", true, false),
            Block { outside: false }
        );
    }
}
