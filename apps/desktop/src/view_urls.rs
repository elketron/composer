//! Where a stage tab loads from, and which navigations are really "open this page as a tab".

use composer_client::Endpoint;
use composer_protocol::DeskTab;

/// `http://host:port` of a workspace base like `http://host:port/w/default`.
pub fn origin(base: &str) -> &str {
    let after_scheme = base.find("://").map_or(0, |i| i + 3);
    match base[after_scheme..].find('/') {
        Some(slash) => &base[..after_scheme + slash],
        None => base,
    }
}

/// The URL a page or changes tab shows (the server's read-only views; the token once, after
/// which the server's cookie carries it). Apps get theirs from `app.open`.
pub fn view_url(endpoint: &Endpoint, tab: &DeskTab) -> Option<String> {
    let path = match tab {
        DeskTab::Page { url } => format!("/view/{}", url.strip_prefix("site://")?),
        DeskTab::Changes { project } => format!("/view/changes/{}", encode(project)),
        _ => return None,
    };
    Some(format!(
        "{}?token={}",
        endpoint.url(&path),
        endpoint.token()
    ))
}

/// The page a clicked link means (`site://...`, or another page's view on this workspace), so it
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

/// A link that leaves the workspace (another site on the web, mail): it opens with the OS,
/// not in the pane. Links within the workspace's server stay in the pane.
pub fn external(link: &str, base: &str) -> bool {
    let web = link.starts_with("http://") || link.starts_with("https://");
    let ours = link.starts_with(origin(base));
    (web && !ours) || link.starts_with("mailto:")
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

    const BASE: &str = "http://127.0.0.1:7420/w/default";

    #[test]
    fn origins_and_views() {
        assert_eq!(origin(BASE), "http://127.0.0.1:7420");
        assert_eq!(origin("http://h:1"), "http://h:1");
        let endpoint = Endpoint::new(BASE, "tok");
        let page = DeskTab::Page {
            url: "site://plans/x.md".into(),
        };
        assert_eq!(
            view_url(&endpoint, &page).unwrap(),
            "http://127.0.0.1:7420/w/default/view/plans/x.md?token=tok"
        );
        let changes = DeskTab::Changes {
            project: "my game".into(),
        };
        assert_eq!(
            view_url(&endpoint, &changes).unwrap(),
            "http://127.0.0.1:7420/w/default/view/changes/my%20game?token=tok"
        );
        assert_eq!(
            view_url(&endpoint, &DeskTab::Terminal { id: "t".into() }),
            None
        );
    }

    #[test]
    fn outside_links_and_dropped_paths() {
        assert!(external("https://example.com/x", BASE));
        assert!(external("mailto:a@b.c", BASE));
        assert!(!external(
            "http://127.0.0.1:7420/w/default/orchestration/",
            BASE
        ));
        assert!(!external("site://plans/x.md", BASE));
        assert!(!external("javascript:alert(1)", BASE));
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
            page_for(
                &format!("{BASE}/view/orchestration/tickets/0178.md#result"),
                BASE
            )
            .as_deref(),
            Some("site://orchestration/tickets/0178.md")
        );
        assert_eq!(page_for(&format!("{BASE}/view/changes/game"), BASE), None);
        assert_eq!(
            page_for(&format!("{BASE}/orchestration/"), BASE),
            None,
            "an app"
        );
        assert_eq!(page_for("https://example.com/view/x", BASE), None);
        assert_eq!(
            page_for("http://127.0.0.1:7420/w/other/view/x.md", BASE),
            None,
            "another workspace"
        );
    }
}
