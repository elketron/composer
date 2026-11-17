//! Which stage tabs a page or project change is about.
//!
//! The server broadcasts `page.changed` (scoped per page URL,
//! `crates/server/src/api/event_scope.rs`) and `project.changed` (scoped per open project). A tab
//! showing one of those must show the new text without the user reloading it — that is the "watch
//! the agent work in the page you asked it to change" half of the stage. This module answers one
//! question and nothing else: *which tabs is this event about*. What a tab then does, and when, is
//! [`crate::refresher`].
//!
//! Page identity is the server's, not a second opinion on it: [`composer_core::SiteUrl`] parses a
//! page URL and [`SiteUrl::candidates`] is the file list a URL resolves to. Two URLs are the same
//! page when their candidates meet, which is exactly how the server decides what a page is — `x`,
//! `x.md`, `x/`, `x#frag` and (for a directory) `x/index.md` are one page, while `x.md` and
//! `x/index.md` are two files, and `justfile` is a file rather than a page. Nothing is matched by
//! prefix, so a tab showing `site://a` is untouched by a change to `site://a/b`.

use composer_core::SiteUrl;
use composer_protocol::{Desk, DeskEntry, DeskTab};

/// The prefix of a page tab's id, as the server derives it (`SessionDesk::key`:
/// `page:<SiteUrl>` of the normalised URL).
const PAGE_ID: &str = "page:";

/// Whether two `site://` URLs are the same page. Anything that is not a page URL is no page, and
/// matches nothing.
pub fn same_page(a: &str, b: &str) -> bool {
    let (Ok(a), Ok(b)) = (SiteUrl::parse(a), SiteUrl::parse(b)) else {
        return false;
    };
    if a.site() != b.site() {
        return false;
    }
    let (mine, theirs) = (a.candidates(), b.candidates());
    mine.iter().any(|file| theirs.contains(file))
}

/// The page a tab shows, as the page URL it can be compared with.
///
/// The id, not the tab's own `url`: the id is the normalised page URL (`SessionDesk::key`), while
/// `url` is whatever the client that opened the tab sent — `site://plans/./x` or ` site://plans/x`
/// parse, `plans/x` does not, and none of them is what a change event names. A page tab whose id
/// names no page (a desk from an older server, or a test fixture) falls back to its `url`.
fn shown_page(entry: &DeskEntry) -> Option<&str> {
    let DeskTab::Page { url } = &entry.tab else {
        return None;
    };
    Some(entry.id.strip_prefix(PAGE_ID).unwrap_or(url))
}

/// The stage tabs showing page `url`, in desk order. A tab of any other kind never matches: an app,
/// a live view or a terminal is a running program, not a page, and reloading one would interrupt it.
pub fn page_tabs(desk: &Desk, url: &str) -> Vec<String> {
    desk.tabs
        .iter()
        .filter(|entry| shown_page(entry).is_some_and(|shown| same_page(shown, url)))
        .map(|entry| entry.id.clone())
        .collect()
}

/// The stage tabs showing the changes of project `project`.
pub fn changes_tabs(desk: &Desk, project: &str) -> Vec<String> {
    desk.tabs
        .iter()
        .filter(|entry| matches!(&entry.tab, DeskTab::Changes { project: p } if p == project))
        .map(|entry| entry.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page tab as the server would have named it: `page:` + the normalised URL.
    fn opened(url: &str) -> (String, DeskTab) {
        (format!("{PAGE_ID}{url}"), DeskTab::Page { url: url.into() })
    }

    fn desk(tabs: &[(&str, DeskTab)], active: Option<&str>) -> Desk {
        Desk {
            tabs: tabs
                .iter()
                .map(|(id, tab)| DeskEntry {
                    id: id.to_string(),
                    tab: tab.clone(),
                    unseen: false,
                })
                .collect(),
            active: active.map(str::to_string),
        }
    }

    #[test]
    fn the_tab_showing_the_page_and_no_other() {
        let (x_id, x_tab) = opened("site://plans/x.md");
        let (other_id, other_tab) = opened("site://memory/projects/composer.md");
        let d = desk(
            &[
                (x_id.as_str(), x_tab),
                (other_id.as_str(), other_tab),
                ("t3", DeskTab::Terminal { id: "t3".into() }),
                (
                    "t4",
                    DeskTab::App {
                        site: "video".into(),
                        project: None,
                    },
                ),
                ("t5", DeskTab::LiveView),
            ],
            Some(x_id.as_str()),
        );
        assert_eq!(page_tabs(&d, "site://plans/x.md"), [x_id.as_str()]);
        assert_eq!(
            page_tabs(&d, "site://plans/x"),
            [x_id.as_str()],
            "the same page, written as the reader would"
        );
        assert_eq!(
            page_tabs(&d, "site://memory/projects/composer.md"),
            [other_id.as_str()],
            "the other open page"
        );
        assert!(
            page_tabs(&d, "site://plans/y.md").is_empty(),
            "no tab shows it"
        );
        assert!(
            page_tabs(&d, "site://plans/index.md").is_empty(),
            "a site's index is neither open page"
        );
        // Exactly one tab matched: an app, a live view and a terminal are never reloaded by a
        // page change.
        assert_eq!(page_tabs(&d, "site://plans/x.md").len(), 1);
    }

    #[test]
    fn a_tab_matches_the_pages_id_not_the_raw_url_it_was_opened_with() {
        // A tab's `url` is whatever its client sent (`open_page` passes it through) and the id is
        // the server's normalised URL; the event names a file. So the id is what reaches the tab.
        let (id, _) = opened("site://plans/x.md");
        let raw = |url: &str| desk(&[(&id, DeskTab::Page { url: url.into() })], Some(&id));
        for opened_as in [
            "plans/x",
            " site://plans/x ",
            "site://plans/./x",
            "site://plans//x",
        ] {
            assert_eq!(
                page_tabs(&raw(opened_as), "site://plans/x.md"),
                [id.as_str()],
                "a tab opened as `{opened_as}` still shows that page"
            );
        }
        // A site's index: its id is `page:site://<site>`, the event names `index.md`.
        let (home_id, home_tab) = opened("site://memory");
        let d = desk(&[(&home_id, home_tab)], Some(home_id.as_str()));
        assert_eq!(page_tabs(&d, "site://memory/index.md"), [home_id.as_str()]);
        // A page tab whose id names no page (an older server) falls back to its own url.
        let legacy = desk(
            &[(
                "0",
                DeskTab::Page {
                    url: "site://plans/x.md".into(),
                },
            )],
            Some("0"),
        );
        assert_eq!(page_tabs(&legacy, "site://plans/x.md"), ["0"]);
    }

    #[test]
    fn the_same_page_however_it_is_written() {
        // The server's own resolution (`SiteUrl::candidates`): every spelling of one page meets
        // there, and two files never do.
        assert!(same_page("site://plans/x", "site://plans/x.md"));
        assert!(same_page("site://plans/x.md", "site://plans/x"));
        assert!(same_page("site://plans/x.md#result", "site://plans/x.md"));
        assert!(same_page("site://plans/x/", "site://plans/x.md"));
        assert!(
            same_page("site://plans/./x", "site://plans/x.md"),
            "a . segment"
        );
        assert!(
            same_page("site://plans//x", "site://plans/x.md"),
            "an empty one"
        );
        assert!(same_page("site://a/b", "site://a/b/index.md"));
        assert!(same_page("site://x/y/index.md", "site://x/y"));
        assert!(same_page("site://memory", "site://memory/index.md"));
        assert!(same_page("site://memory/index", "site://memory/index.md"));
        assert!(
            !same_page("site://plans/x.md", "site://plans/x.json"),
            "a file"
        );
        assert!(
            !same_page("site://plans/x.md", "site://other/x.md"),
            "a site"
        );
        assert!(
            !same_page("site://a", "site://a.md"),
            "a site's index is not a file at its root"
        );
        assert!(
            !same_page("site://a/b.md", "site://a/b/index.md"),
            "two files, whatever they render"
        );
        // `justfile` has no extension but is no page either (`SiteUrl::candidates`).
        assert!(same_page("site://x/justfile", "site://x/justfile"));
        assert!(!same_page("site://x/justfile", "site://x/justfile.md"));
        // Not page URLs at all: a view's URL, a relative path, whitespace, nothing.
        for other in [
            "http://127.0.0.1:7420/w/default/view/x.md",
            "/view/x.md",
            "plans/x.md",
            "",
            "site://",
        ] {
            assert!(!same_page("site://plans/x.md", other), "{other}");
            assert!(!same_page(other, "site://plans/x.md"), "{other}");
        }
    }

    #[test]
    fn a_prefix_url_never_matches_the_wrong_tab() {
        let (index, index_tab) = opened("site://a");
        let (short, short_tab) = opened("site://a/b.md");
        let (long, long_tab) = opened("site://a/b/c.md");
        let d = desk(
            &[
                (index.as_str(), index_tab),
                (short.as_str(), short_tab),
                (long.as_str(), long_tab),
            ],
            Some(index.as_str()),
        );
        assert_eq!(page_tabs(&d, "site://a/b.md"), [short.as_str()]);
        assert_eq!(page_tabs(&d, "site://a/b/c.md"), [long.as_str()]);
        assert_eq!(page_tabs(&d, "site://a/index.md"), [index.as_str()]);
        assert_eq!(
            page_tabs(&d, "site://a/b"),
            [short.as_str()],
            "written without the md"
        );
        // The prefixes, and a name that merely starts the same way.
        assert!(page_tabs(&d, "site://a.md").is_empty());
        assert!(page_tabs(&d, "site://a/bc.md").is_empty());
        assert!(page_tabs(&d, "site://a/b/c/d.md").is_empty(), "one deeper");
    }

    #[test]
    fn changes_tabs_only_of_that_project() {
        let (page_id, page_tab) = opened("site://plans/x.md");
        let d = desk(
            &[
                (
                    "c1",
                    DeskTab::Changes {
                        project: "game".into(),
                    },
                ),
                (
                    "c2",
                    DeskTab::Changes {
                        project: "other".into(),
                    },
                ),
                (page_id.as_str(), page_tab),
            ],
            Some("c1"),
        );
        assert_eq!(changes_tabs(&d, "game"), ["c1"]);
        assert!(changes_tabs(&d, "gone").is_empty());
    }
}
