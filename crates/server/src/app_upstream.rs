//! The user's own local web servers (`[apps.<name>]`), the one place each may be reached by the
//! proxy (`api/app_proxy.rs`): loopback only, checked when config.toml is read.

use std::collections::BTreeMap;

use composer_protocol::http::AppInfo;
use reqwest::Url;

use crate::config::Config;
use crate::config_sections::AppConfig;
use crate::viewer_upstream::is_loopback;

/// Where composer serves the apps: `/apps/<name>/...`.
pub const SERVED_AT: &str = "/apps";

/// A configured app: where its requests go.
#[derive(Debug, Clone, PartialEq)]
pub struct AppUpstream {
    name: String,
    /// The app's address, `http://127.0.0.1:4477/` (its path is `prefix`).
    base: Url,
    /// The path the app is under on its server, without a trailing slash (`""`, `/canvas`).
    prefix: String,
}

/// Whether `name` can be an app's name: a path segment of letters, digits, `-` and `_`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// Every `[apps.*]` usable, or why one is not: the error a config.toml with it fails to load with.
pub fn check(apps: &BTreeMap<String, AppConfig>) -> Result<(), String> {
    apps.iter()
        .try_for_each(|(name, app)| AppUpstream::new(name, app).map(|_| ()))
}

/// The configured apps as clients see them (`GET /api/info`), by name.
pub fn listed(config: &Config) -> Vec<AppInfo> {
    config
        .apps
        .iter()
        .filter(|(name, app)| AppUpstream::new(name, app).is_ok())
        .map(|(name, app)| AppInfo {
            name: name.clone(),
            title: match app.title.trim() {
                "" => name.clone(),
                title => title.to_string(),
            },
        })
        .collect()
}

impl AppUpstream {
    pub fn new(name: &str, app: &AppConfig) -> Result<Self, String> {
        let setting = format!("[apps.{name}]");
        if !valid_name(name) {
            return Err(format!(
                "{setting}: an app's name may only have letters, digits, `-` and `_`"
            ));
        }
        let text = app.url.trim();
        let mut base = Url::parse(text).map_err(|e| format!("{setting} url `{text}`: {e}"))?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(format!("{setting} url `{text}` must be an http(s) URL"));
        }
        if !is_loopback(&base) {
            return Err(format!(
                "{setting} url `{text}` is not a loopback address (127.0.0.1, ::1 or localhost); \
                 only those are allowed"
            ));
        }
        if !base.username().is_empty() || base.password().is_some() {
            return Err(format!(
                "{setting} url `{text}` must not hold a user or password"
            ));
        }
        base.set_query(None);
        base.set_fragment(None);
        let prefix = base.path().trim_end_matches('/').to_string();
        Ok(Self {
            name: name.to_string(),
            base,
            prefix,
        })
    }

    /// The app `name` as the config has it; `None` when there is none (or it is unusable).
    pub fn find(config: &Config, name: &str) -> Option<Self> {
        Self::new(name, config.apps.get(name)?).ok()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where composer serves it: `/apps/<name>` (its page is that with a `/`).
    pub fn served_at(&self) -> String {
        format!("{SERVED_AT}/{}", self.name)
    }

    fn same_origin(&self, url: &Url) -> bool {
        url.scheme() == self.base.scheme()
            && url.host_str() == self.base.host_str()
            && url.port_or_known_default() == self.base.port_or_known_default()
    }

    /// The upstream URL for `rest` (the request's path below `/apps/<name>`, starting with `/`),
    /// with `query`; `ws` picks the WebSocket scheme. `None` when `rest` isn't a plain path
    /// ([`clean_rest`]) or the URL would not be under the app.
    pub fn url(&self, rest: &str, query: Option<&str>, ws: bool) -> Option<String> {
        if !clean_rest(rest) {
            return None;
        }
        let wanted = format!("{}{rest}", self.prefix);
        let mut url = self.base.clone();
        url.set_path(&wanted);
        if url.path() != wanted || !self.same_origin(&url) {
            return None;
        }
        url.set_query(query.filter(|q| !q.is_empty()));
        if ws {
            let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
            url.set_scheme(scheme).ok();
        }
        Some(url.into())
    }

    /// Where composer serves `location` (an upstream redirect target): an absolute path, or an
    /// absolute URL of this very app, under the app's prefix, as the same path under
    /// `/apps/<name>`. `None` for anything else (a relative path, another origin), which the proxy
    /// leaves as it is.
    pub fn local_path(&self, location: &str) -> Option<String> {
        if location.contains('\\')
            || location.starts_with("//")
            || location
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return None;
        }
        let url = if location.starts_with('/') {
            self.base.join(location).ok()?
        } else {
            Url::parse(location).ok()?
        };
        if !self.same_origin(&url) || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let rest = url.path().strip_prefix(&self.prefix)?;
        if !(rest.is_empty() || rest.starts_with('/')) {
            return None;
        }
        let rest = if rest.is_empty() { "/" } else { rest };
        Some(match url.query() {
            Some(q) => format!("{}{rest}?{q}", self.served_at()),
            None => format!("{}{rest}", self.served_at()),
        })
    }
}

/// Whether `rest` (a request's path below `/apps/<name>`, as the client sent it) is one the proxy
/// forwards: `/`, or `/` and a [`plain_path`]. Anything else could leave the app once the URL
/// parser resolves it (`/a/../../x`, `/%2e%2e/x`).
pub fn clean_rest(rest: &str) -> bool {
    rest.strip_prefix('/').is_some_and(plain_path)
}

/// A path relative to an app's root: empty, or segments that are not empty, `.` or `..` (a
/// trailing `/` is fine), with no backslash, control character or whitespace, and no
/// percent-encoded dot, slash or backslash (`%2e`, `%2f`, `%5c`, any case).
fn plain_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    let lower = path.to_ascii_lowercase();
    let segments = path.strip_suffix('/').unwrap_or(path);
    !segments.is_empty()
        && !path
            .chars()
            .any(|c| c == '\\' || c.is_control() || c.is_whitespace())
        && !["%2e", "%2f", "%5c"].iter().any(|bad| lower.contains(bad))
        && segments
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | ".."))
}

/// An app tab's `path` (relative to the app's root, with an optional `?query` and `#fragment`)
/// as the desk keeps it: trimmed, without a leading `/`; refused when its path part isn't a
/// [`plain_path`].
pub fn desk_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    let path = path.strip_prefix('/').unwrap_or(path);
    let end = path.find(['?', '#']).unwrap_or(path.len());
    let refused = path.chars().any(|c| c.is_control() || c.is_whitespace());
    if refused || !plain_path(&path[..end]) {
        return Err(format!(
            "`{path}` isn't a path in an app (relative to its root, no `..`)"
        ));
    }
    Ok(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(url: &str) -> AppConfig {
        AppConfig {
            url: url.into(),
            title: String::new(),
        }
    }

    #[test]
    fn config_takes_loopback_apps_only() {
        let config = Config::parse(
            "[apps.canvas]\nurl = \"http://127.0.0.1:4477\"\ntitle = \"Canvas\"\n\n\
             [apps.b]\nurl = \"http://[::1]:5000/sub/\"\n\n[apps.c]\nurl = \"http://localhost:1\"\n",
        )
        .unwrap();
        assert_eq!(config.apps["canvas"].title, "Canvas");
        assert_eq!(Config::parse(&config.to_toml()).unwrap(), config);
        assert!(AppUpstream::find(&config, "b").is_some());
        assert_eq!(AppUpstream::find(&config, "nope"), None);

        for (text, why) in [
            ("[apps.x]\nurl = \"http://192.168.1.5:4477\"\n", "loopback"),
            ("[apps.x]\nurl = \"http://example.com\"\n", "loopback"),
            (
                "[apps.x]\nurl = \"http://127.0.0.1.example.com\"\n",
                "loopback",
            ),
            ("[apps.x]\nurl = \"ftp://127.0.0.1\"\n", "http(s)"),
            ("[apps.x]\nurl = \"127.0.0.1:4477\"\n", "url"),
            ("[apps.x]\nurl = \"http://u:p@127.0.0.1:1\"\n", "user"),
            ("[apps.x]\ntitle = \"no url\"\n", "url"),
            ("[apps.\"a b\"]\nurl = \"http://127.0.0.1:1\"\n", "name"),
            ("[apps.\"a.b\"]\nurl = \"http://127.0.0.1:1\"\n", "name"),
        ] {
            let error = Config::parse(text).unwrap_err().to_string();
            assert!(error.contains(why), "{text}: {error}");
        }
        let error = Config::parse("[apps.x]\nurl = \"http://10.0.0.1\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("[apps.x]"), "{error}");
    }

    #[test]
    fn urls_stay_under_the_app() {
        let up = AppUpstream::new("c", &app("http://127.0.0.1:4477")).unwrap();
        assert_eq!(up.url("/", None, false).unwrap(), "http://127.0.0.1:4477/");
        assert_eq!(
            up.url("/api/x", Some("a=1"), false).unwrap(),
            "http://127.0.0.1:4477/api/x?a=1"
        );
        assert_eq!(up.url("/ws", None, true).unwrap(), "ws://127.0.0.1:4477/ws");
        let sub = AppUpstream::new("c", &app("http://127.0.0.1:4477/sub/")).unwrap();
        assert_eq!(
            sub.url("/a/", None, false).unwrap(),
            "http://127.0.0.1:4477/sub/a/"
        );
        for rest in ["/../x", "/a/../../x", "/%2e%2e/x", "/a%2fb", "//x", "", "x"] {
            assert_eq!(sub.url(rest, None, false), None, "{rest}");
        }
    }

    #[test]
    fn only_plain_paths_below_an_app() {
        for rest in ["/", "/a", "/a/", "/a/b.js", "/a%20b", "/api/boards/3?x"] {
            assert!(clean_rest(rest), "{rest}");
        }
        for rest in [
            "",
            "x",
            "//",
            "//a",
            "/a//b",
            "/..",
            "/../x",
            "/a/..",
            "/./a",
            "/%2e%2e/x",
            "/%2E%2e/x",
            "/.%2e/x",
            "/a%2fb",
            "/a%2Fb",
            "/a%5cb",
            "/a\\b",
        ] {
            assert!(!clean_rest(rest), "{rest}");
        }
    }

    #[test]
    fn a_desk_path_is_relative_to_the_app() {
        assert_eq!(desk_path("").unwrap(), "");
        assert_eq!(desk_path(" /board/3 ").unwrap(), "board/3");
        assert_eq!(desk_path("board/?x=1#y").unwrap(), "board/?x=1#y");
        assert_eq!(desk_path("?b=../x").unwrap(), "?b=../x");
        for bad in [
            "../x",
            "a/../../b",
            "a//b",
            "%2e%2e/x",
            "a\\b",
            "a b",
            "//x",
        ] {
            assert!(desk_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn redirects_come_back_under_the_app() {
        let up = AppUpstream::new("c", &app("http://127.0.0.1:4477/sub")).unwrap();
        for (location, served) in [
            ("/sub", "/apps/c/"),
            ("/sub/", "/apps/c/"),
            ("/sub/x?y=1", "/apps/c/x?y=1"),
            ("http://127.0.0.1:4477/sub/z", "/apps/c/z"),
        ] {
            assert_eq!(
                up.local_path(location).as_deref(),
                Some(served),
                "{location}"
            );
        }
        for location in [
            "x/y",
            "//evil.example/sub",
            "http://evil.example/sub",
            "http://127.0.0.1:4478/sub",
            "/subway",
            "/other",
        ] {
            assert_eq!(up.local_path(location), None, "{location}");
        }
    }
}
