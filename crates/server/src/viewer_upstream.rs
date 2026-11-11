//! Where wayai's viewer is, as `[viewer]` and `[mcp.wayai]` say: the one place the proxy
//! (`api/viewer_proxy.rs`) may connect to. Loopback only, for now.

use std::net::IpAddr;
use std::path::PathBuf;

use reqwest::Url;

use crate::config::Config;
use crate::tools::expand_home;

/// The MCP server whose address the viewer is taken from when `[viewer] url` is not set.
const WAYAI_SERVER: &str = "wayai";

#[derive(Debug, Clone, PartialEq)]
pub struct ViewerUpstream {
    /// The viewer page, without a trailing slash in its path (`http://127.0.0.1:7878/view`).
    page: Url,
    /// The file holding the bearer token the viewer asks for (`[viewer] bearer_file`, else
    /// `[mcp.wayai]`'s when the viewer is at that server's address).
    bearer_file: Option<PathBuf>,
}

/// Where composer serves the viewer, under a workspace's routes.
const SERVED_AT: &str = "/view";

/// The trimmed content of a bearer token file; the error names the file.
pub(crate) fn read_bearer(file: &std::path::Path) -> Result<String, String> {
    let token = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read the bearer token file {}: {e}", file.display()))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(format!("the bearer token file {} is empty", file.display()));
    }
    Ok(token.to_string())
}

impl ViewerUpstream {
    /// The upstream the config names: `[viewer] url`, else `[mcp.wayai]`'s address with path
    /// `/view`. `Ok(None)` when neither is set; `Err` (why) for an address that isn't usable,
    /// which counts as no viewer.
    pub fn from_config(config: &Config) -> Result<Option<Self>, String> {
        let wayai = config.mcp.get(WAYAI_SERVER);
        let explicit = config.viewer.url.trim();
        let derived = wayai
            .map(|mcp| mcp.url.trim())
            .filter(|url| !url.is_empty());
        let mut upstream = if !explicit.is_empty() {
            Self::new(explicit, "[viewer] url")?
        } else {
            let Some(derived) = derived else {
                return Ok(None);
            };
            let mut url =
                Url::parse(derived).map_err(|e| format!("[mcp.wayai] url `{derived}`: {e}"))?;
            url.set_path("/view");
            url.set_query(None);
            url.set_fragment(None);
            Self::new(url.as_str(), "[mcp.wayai] url")?
        };
        // The viewer's own file; else wayai's, only for a viewer at wayai's own address: its
        // token goes to no other program.
        let own = config.viewer.bearer_file.trim();
        let wayais = wayai
            .filter(|mcp| Url::parse(mcp.url.trim()).is_ok_and(|url| upstream.same_origin(&url)))
            .map(|mcp| mcp.bearer_file.trim())
            .unwrap_or("");
        upstream.bearer_file = [own, wayais]
            .into_iter()
            .find(|file| !file.is_empty())
            .map(expand_home);
        Ok(Some(upstream))
    }

    fn same_origin(&self, url: &Url) -> bool {
        url.scheme() == self.page.scheme()
            && url.host_str() == self.page.host_str()
            && url.port_or_known_default() == self.page.port_or_known_default()
    }

    /// The bearer token to send upstream, read from its file now (the viewer may have restarted
    /// with a new one): `None` when no file is configured, `Err` (naming the file) when it cannot
    /// be read.
    pub fn bearer(&self) -> Result<Option<String>, String> {
        self.bearer_file.as_deref().map(read_bearer).transpose()
    }

    /// Logs why a configured viewer is refused; the proxy then answers as if there were none.
    pub fn warn_if_refused(config: &Config) {
        if let Err(why) = Self::from_config(config) {
            tracing::warn!("no viewer: {why}");
        }
    }

    fn new(text: &str, setting: &str) -> Result<Self, String> {
        let mut page = Url::parse(text).map_err(|e| format!("{setting} `{text}`: {e}"))?;
        if !matches!(page.scheme(), "http" | "https") {
            return Err(format!("{setting} `{text}` must be an http(s) URL"));
        }
        let host = page.host_str().unwrap_or("").trim_matches(['[', ']']);
        let loopback = host.eq_ignore_ascii_case("localhost")
            || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
        if !loopback {
            return Err(format!(
                "{setting} `{text}` is not a loopback address (127.0.0.1, ::1 or localhost); \
                 only those are allowed for now"
            ));
        }
        page.set_query(None);
        page.set_fragment(None);
        let path = page.path().trim_end_matches('/').to_string();
        page.set_path(&path);
        Ok(Self {
            page,
            bearer_file: None,
        })
    }

    /// The upstream URL for `rest` (`""`, `/`, `/ws`, `/annotations`...) under the viewer page,
    /// with `query` (already without composer's own parameters). `ws` picks the WebSocket scheme.
    /// `None` when the URL would not be the viewer page or under it (a `rest` with `..` or an
    /// encoded dot or slash, which the URL parser resolves): the caller refuses those before
    /// asking, and this is the second lock.
    pub fn url(&self, rest: &str, query: Option<&str>, ws: bool) -> Option<String> {
        let wanted = format!("{}{rest}", self.page.path());
        let mut url = self.page.clone();
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

    /// Where composer serves `location` (an upstream redirect target), as a path under `/view`
    /// with its query: only for a plain absolute path, or an absolute URL of this very upstream,
    /// that is the viewer page or under it. Anything else (`//host`, `/\host`, another origin, a
    /// relative path, a path outside the viewer) is `None`, and the redirect loses its target.
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
            self.page.join(location).ok()?
        } else {
            Url::parse(location).ok()?
        };
        if !self.same_origin(&url) || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let rest = url.path().strip_prefix(self.page.path())?;
        if !(rest.is_empty() || rest.starts_with('/')) {
            return None;
        }
        Some(match url.query() {
            Some(q) => format!("{SERVED_AT}{rest}?{q}"),
            None => format!("{SERVED_AT}{rest}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> Config {
        Config::parse(text).unwrap()
    }

    #[test]
    fn explicit_url_wins_and_wayai_is_the_fallback() {
        let c = config(
            "[viewer]\nurl = \"http://localhost:9000/view/\"\n[mcp.wayai]\nurl = \"http://127.0.0.1:7878/mcp\"\n",
        );
        let up = ViewerUpstream::from_config(&c).unwrap().unwrap();
        assert_eq!(
            up.url("/ws", Some("a=1"), false).unwrap(),
            "http://localhost:9000/view/ws?a=1"
        );
        let c = config("[mcp.wayai]\nurl = \"http://127.0.0.1:7878/mcp\"\n");
        let up = ViewerUpstream::from_config(&c).unwrap().unwrap();
        assert_eq!(
            up.url("", None, false).unwrap(),
            "http://127.0.0.1:7878/view"
        );
        assert_eq!(
            up.url("/ws", None, true).unwrap(),
            "ws://127.0.0.1:7878/view/ws"
        );
        assert_eq!(ViewerUpstream::from_config(&config("")).unwrap(), None);
    }

    #[test]
    fn only_loopback() {
        for url in [
            "http://192.168.1.5:7878/view",
            "http://example.com/view",
            "ftp://127.0.0.1/view",
        ] {
            let c = config(&format!("[viewer]\nurl = \"{url}\"\n"));
            assert!(ViewerUpstream::from_config(&c).is_err(), "{url}");
        }
        let c = config("[viewer]\nurl = \"http://[::1]:7878/view\"\n");
        assert!(ViewerUpstream::from_config(&c).unwrap().is_some());
    }

    #[test]
    fn urls_stay_under_the_viewer() {
        let c = config("[viewer]\nurl = \"http://127.0.0.1:7878/view\"\n");
        let up = ViewerUpstream::from_config(&c).unwrap().unwrap();
        for rest in [
            "/recordings/../../mcp",
            "/..",
            "/../mcp",
            "/recordings/%2e%2e/%2E%2e/mcp",
            "/./ws",
            "/recordings/a/..",
        ] {
            assert_eq!(up.url(rest, None, false), None, "{rest}");
        }
        assert!(up.url("/recordings/a/b.webm", None, false).is_some());
    }

    #[test]
    fn redirects_stay_on_the_viewer() {
        let c = config("[viewer]\nurl = \"http://127.0.0.1:7878/screen\"\n");
        let up = ViewerUpstream::from_config(&c).unwrap().unwrap();
        for (location, served) in [
            ("/screen", "/view"),
            ("/screen/", "/view/"),
            ("/screen/recordings/a?x=1", "/view/recordings/a?x=1"),
            ("http://127.0.0.1:7878/screen/text/3", "/view/text/3"),
        ] {
            assert_eq!(
                up.local_path(location).as_deref(),
                Some(served),
                "{location}"
            );
        }
        for location in [
            "//evil.example/screen",
            "/\\evil.example/screen",
            "\\\\evil.example/screen",
            "/screen\\..\\mcp",
            "http://evil.example/screen",
            "http://127.0.0.1:7879/screen",
            "https://127.0.0.1:7878/screen",
            "http://user@127.0.0.1:7878/screen",
            "http://127.0.0.1:7878@evil.example/screen",
            "javascript:alert(1)",
            "/mcp",
            "/screenshots",
            "/screen/../mcp",
            "/screen/%2e%2e/mcp",
            "recordings/a",
            "",
            " /screen",
        ] {
            assert_eq!(up.local_path(location), None, "{location}");
        }
    }

    #[test]
    fn the_bearer_file_is_the_viewers_or_wayais_at_the_same_address() {
        let dir = tempfile::tempdir().unwrap();
        let (own, wayai) = (dir.path().join("own"), dir.path().join("wayai"));
        std::fs::write(&own, " own-token\n").unwrap();
        std::fs::write(&wayai, "wayai-token\n").unwrap();
        let (own, wayai) = (own.display(), wayai.display());
        let bearer = |text: String| {
            ViewerUpstream::from_config(&config(&text))
                .unwrap()
                .unwrap()
                .bearer()
        };
        let mcp = format!(
            "[mcp.wayai]\nurl = \"http://127.0.0.1:7878/mcp\"\nbearer_file = \"{wayai}\"\n"
        );
        // Derived from [mcp.wayai]: its file.
        assert_eq!(bearer(mcp.clone()).unwrap().as_deref(), Some("wayai-token"));
        // The viewer's own file wins.
        assert_eq!(
            bearer(format!("[viewer]\nbearer_file = \"{own}\"\n{mcp}"))
                .unwrap()
                .as_deref(),
            Some("own-token")
        );
        // An explicit viewer at wayai's address: wayai's file; at another address: none.
        assert_eq!(
            bearer(format!(
                "[viewer]\nurl = \"http://127.0.0.1:7878/view\"\n{mcp}"
            ))
            .unwrap()
            .as_deref(),
            Some("wayai-token")
        );
        assert_eq!(
            bearer(format!(
                "[viewer]\nurl = \"http://127.0.0.1:9000/view\"\n{mcp}"
            ))
            .unwrap(),
            None
        );
        // No file configured: no header. A file that is missing or empty: an error naming it.
        assert_eq!(
            bearer("[viewer]\nurl = \"http://127.0.0.1:7878/view\"\n".into()).unwrap(),
            None
        );
        let missing = dir.path().join("missing");
        let error = bearer(format!(
            "[viewer]\nurl = \"http://127.0.0.1:7878/view\"\nbearer_file = \"{}\"\n",
            missing.display()
        ))
        .unwrap_err();
        assert!(error.contains(&missing.display().to_string()), "{error}");
        std::fs::write(&missing, "\n").unwrap();
        let error = bearer(format!(
            "[viewer]\nurl = \"http://127.0.0.1:7878/view\"\nbearer_file = \"{}\"\n",
            missing.display()
        ))
        .unwrap_err();
        assert!(error.contains("is empty"), "{error}");
    }
}
