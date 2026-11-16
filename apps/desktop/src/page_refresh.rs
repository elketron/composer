//! Which stage tabs a page or project change is about, and when each of them reloads.
//!
//! The server broadcasts `page.changed` (scoped per page URL,
//! `crates/server/src/api/event_scope.rs`) and `project.changed` (scoped per open project). A tab
//! showing one of those must show the new text without the user reloading it — that is the "watch
//! the agent work in the page you asked it to change" half of the stage. Everything that decides
//! *which* tab, *how soon*, and *whether now at all* is here, as functions over the desk, so it is
//! tested without a browser (as `pick_lists` is for lists); `Shell` only drives what they say.
//!
//! Matching follows the server rather than strings. A page tab holds a URL as it was written
//! (`site://plans/x`, `site://x.md`, `site://x/index.md`) while a change event names the file that
//! changed (`site://x.md`), and the server resolves all of those spellings to one page
//! (`SiteUrl::candidates`, `crates/core/src/site_url.rs`). So a URL is reduced to the page it means
//! — the `.md` of a page, the `index` of a directory, and a site's own index all disappear — and two
//! URLs are the same page when those agree. Nothing is matched by prefix, so `site://a` never
//! reaches a tab showing `site://a/b` (as `api/glob.rs` matches a `/**` pattern per segment, a
//! prefix is not a match).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use composer_protocol::{Desk, DeskTab};

/// How long a tab waits after a change before it reloads. One agent edit can fire several events
/// for one page, and a worker writing twenty pages must not reload twenty iframes in a burst: the
/// changes for a tab gather here and become one reload. (The server's filewatchers settle and
/// debounce the same way for the same reason, ticket 0119.)
pub const SETTLE: Duration = Duration::from_millis(400);

/// How many of those waits a reload may be held back for a reader who is mid-drag or mid-selection
/// in the page, before it is applied anyway. A page swapping under a held mouse button is its own
/// small bug; a change that arrived is never dropped, so the wait is bounded.
const MAX_HELD: u8 = 4;

/// A page URL as a tab or a change event names it, reduced to what it means: `site://<site>` plus a
/// path with no fragment, no `.md`, no trailing `index`.
struct PageRef<'a> {
    site: &'a str,
    /// Normalised: no surrounding slashes; a site's homepage is the empty path.
    path: &'a str,
}

impl<'a> PageRef<'a> {
    /// `None` for anything that is not a page URL, so it matches no tab.
    fn parse(url: &'a str) -> Option<Self> {
        let rest = url.strip_prefix("site://")?;
        let (rest, _) = rest.split_once('#').unwrap_or((rest, ""));
        let (site, path) = rest.split_once('/').unwrap_or((rest, ""));
        if site.is_empty() {
            return None;
        }
        let path = path.trim_matches('/');
        // A page's extension and a directory's `index` are not part of the page's name; any other
        // extension is, because it is a different file (`.json`, a `justfile`).
        let path = path.strip_suffix(".md").unwrap_or(path);
        let path = path.strip_suffix("/index").unwrap_or(path);
        let path = if path == "index" { "" } else { path };
        Some(Self { site, path })
    }

    /// The names this URL could have been written as, for asking the server about it: the file it
    /// resolves to is what a change event names, and a path with no extension resolves to either of
    /// two files (`SiteUrl::candidates`).
    fn spellings(&self) -> Vec<String> {
        let base = format!("site://{}", self.site);
        if self.path.is_empty() {
            return vec![format!("{base}/index.md")];
        }
        let full = format!("{base}/{}", self.path);
        let last = self
            .path
            .rsplit_once('/')
            .map_or(self.path, |(_, last)| last);
        if last.contains('.') {
            vec![full]
        } else {
            vec![format!("{full}.md"), format!("{full}/index.md")]
        }
    }
}

/// Whether two `site://` URLs are the same page. Anything that isn't one matches nothing.
pub fn same_page(a: &str, b: &str) -> bool {
    match (PageRef::parse(a), PageRef::parse(b)) {
        (Some(a), Some(b)) => a.site == b.site && a.path == b.path,
        _ => false,
    }
}

/// The stage tabs showing page `url`, in desk order. A tab of any other kind never matches: an app,
/// a live view or a terminal is a running program, not a page, and reloading one would interrupt it.
pub fn page_tabs(desk: &Desk, url: &str) -> Vec<String> {
    desk.tabs
        .iter()
        .filter(|entry| match &entry.tab {
            DeskTab::Page { url: shown } => same_page(shown, url),
            _ => false,
        })
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

/// The URLs to ask the server to tell us about (`subscribe`): those events reach only a connection
/// that asked for the URL, so a window that never asks never learns that a page changed. Exactly
/// the pages on the stage and the names each could have been written as — never a site-wide
/// `site://x/**`, because one open page shouldn't pull every change in its site over the wire.
pub fn page_patterns(desk: &Desk) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in &desk.tabs {
        let DeskTab::Page { url } = &entry.tab else {
            continue;
        };
        for name in PageRef::parse(url).into_iter().flat_map(|p| p.spellings()) {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out.sort();
    out
}

/// Which tabs need a reload and when: the coalescing window (`SETTLE`) for the tab on screen, and
/// the mark a hidden tab carries until it is shown. It knows tab ids only — what a tab shows and
/// whether it has a pane is the window's business — so none of it needs a browser.
#[derive(Default)]
pub struct Refresher {
    /// Tab id → (the earliest it reloads, how often a reload was held back for the reader).
    pending: HashMap<String, (Instant, u8)>,
    /// Tabs whose content changed while they were hidden.
    marked: HashSet<String>,
}

impl Refresher {
    /// Tab `tab` shows something that just changed. The tab on screen waits `SETTLE` first, so a
    /// burst of events for one page is one reload; any other is only marked, and reloads when it is
    /// shown — no hidden pane reloads a browser for something nobody is looking at.
    pub fn changed(&mut self, tab: &str, shown: Option<&str>, now: Instant) {
        if Some(tab) == shown {
            self.marked.remove(tab);
            self.pending.insert(tab.to_string(), (now + SETTLE, 0));
        } else {
            self.marked.insert(tab.to_string());
        }
    }

    /// Whether anything is waiting: the window arms its timer only while this holds.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Tabs whose wait has passed, in a stable order. A tab stays pending until the window says it
    /// reloaded it or held it back for the reader, so nothing is lost between two calls.
    pub fn due(&self, now: Instant) -> Vec<String> {
        let mut due: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, (at, _))| *at <= now)
            .map(|(id, _)| id.clone())
            .collect();
        due.sort_unstable();
        due
    }

    /// Whether to wait another round for `tab` because the reader is mid-interaction in it.
    pub fn may_hold(&self, tab: &str) -> bool {
        self.pending
            .get(tab)
            .is_some_and(|(_, held)| *held < MAX_HELD)
    }

    /// The reader is still mid-interaction: wait `SETTLE` from now again (bounded by `may_hold`).
    pub fn held(&mut self, tab: &str, now: Instant) {
        if let Some((at, held)) = self.pending.get_mut(tab) {
            *at = now + SETTLE;
            *held += 1;
        }
    }

    /// The window loaded `tab` again: it is current.
    pub fn reloaded(&mut self, tab: &str) {
        self.pending.remove(tab);
        self.marked.remove(tab);
    }

    /// `tab` is the shown one now: whether it must load, because its page changed while it was
    /// hidden. The mark is consumed either way, so a tab is asked once. A tab already on its way
    /// (`due`) is not touched: showing it must not double up.
    pub fn focus(&mut self, tab: &str) -> bool {
        self.marked.remove(tab)
    }

    /// Whether to hint on the tab that its text is older than the page. Only hidden tabs are ever
    /// marked, so this never appears on the page someone is reading.
    pub fn is_stale(&self, tab: &str) -> bool {
        self.marked.contains(tab)
    }

    /// Only tabs still on the desk keep a reload or a mark.
    pub fn keep(&mut self, tabs: &HashSet<&str>) {
        self.pending.retain(|id, _| tabs.contains(id.as_str()));
        self.marked.retain(|id| tabs.contains(id.as_str()));
    }

    /// A new connection: nothing on screen is known to be current or not.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.marked.clear();
    }
}

#[cfg(test)]
mod tests {
    use composer_protocol::DeskEntry;

    use super::*;

    fn page(url: &str) -> DeskTab {
        DeskTab::Page { url: url.into() }
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
        let d = desk(
            &[
                ("t1", page("site://plans/x.md")),
                ("t2", page("site://memory/projects/composer.md")),
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
            Some("t1"),
        );
        assert_eq!(page_tabs(&d, "site://plans/x.md"), ["t1"]);
        assert_eq!(
            page_tabs(&d, "site://plans/x"),
            ["t1"],
            "the same page, written as the reader would"
        );
        assert_eq!(
            page_tabs(&d, "site://memory/projects/composer.md"),
            ["t2"],
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
        // The one match is the page's tab: an app, a live view and a terminal are never reloaded
        // by a page change.
        assert_eq!(page_tabs(&d, "site://plans/x.md").len(), 1);
    }

    #[test]
    fn the_same_page_however_it_is_written() {
        // A tab's URL and an event's URL come from different places and are written differently:
        // the extension, the fragment, a directory's index.
        assert!(same_page("site://plans/x", "site://plans/x.md"));
        assert!(same_page("site://plans/x.md", "site://plans/x"));
        assert!(same_page("site://plans/x.md#result", "site://plans/x.md"));
        assert!(same_page("site://plans/x/", "site://plans/x.md"));
        assert!(same_page("site://a/b", "site://a/b/index.md"));
        assert!(same_page("site://memory", "site://memory/index.md"));
        assert!(same_page("site://memory/index", "site://memory/index.md"));
        assert!(same_page("site://x/y/index.md", "site://x/y"));
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
        assert!(!same_page("site://x/index.md", "site://x/other.md"));
        // Not a page URL at all: a view's URL, a relative path, nothing.
        for other in [
            "http://127.0.0.1:7420/w/default/view/x.md",
            "/view/x.md",
            "",
            "site://",
        ] {
            assert!(!same_page("site://plans/x.md", other), "{other}");
            assert!(!same_page(other, "site://plans/x.md"), "{other}");
        }
    }

    #[test]
    fn a_prefix_url_never_matches_the_wrong_tab() {
        let d = desk(
            &[
                ("index", page("site://a")),
                ("short", page("site://a/b.md")),
                ("long", page("site://a/b/c.md")),
            ],
            Some("index"),
        );
        assert_eq!(page_tabs(&d, "site://a/b.md"), ["short"]);
        assert_eq!(page_tabs(&d, "site://a/b/c.md"), ["long"]);
        assert_eq!(page_tabs(&d, "site://a/index.md"), ["index"]);
        assert_eq!(
            page_tabs(&d, "site://a/b"),
            ["short"],
            "written without the md"
        );
        // The prefixes, and a name that merely starts the same way.
        assert!(page_tabs(&d, "site://a.md").is_empty());
        assert!(page_tabs(&d, "site://a/bc.md").is_empty());
        assert!(page_tabs(&d, "site://a/b/c/d.md").is_empty(), "one deeper");
    }

    #[test]
    fn changes_tabs_only_of_that_project() {
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
                ("p", page("site://plans/x.md")),
            ],
            Some("c1"),
        );
        assert_eq!(changes_tabs(&d, "game"), ["c1"]);
        assert!(changes_tabs(&d, "gone").is_empty());
    }

    #[test]
    fn subscribing_names_the_opened_pages_in_every_spelling() {
        let d = desk(
            &[
                ("t1", page("site://plans/x.md")),
                ("t2", page("site://plans/x")),
                ("t3", page("site://memory/y")),
                ("t4", DeskTab::Terminal { id: "t".into() }),
                ("t5", page("site://ideas")),
                ("t6", page("site://cad/parts.json")),
            ],
            None,
        );
        // Sorted and deduped: the extension-less page asked every way the server could name the
        // file, a site as its `index.md`, another file by its own name, and nothing for a tab that
        // is not a page.
        assert_eq!(
            page_patterns(&d),
            [
                "site://cad/parts.json",
                "site://ideas/index.md",
                "site://memory/y.md",
                "site://memory/y/index.md",
                "site://plans/x.md",
                "site://plans/x/index.md",
            ]
        );
        assert_eq!(
            page_patterns(&Desk {
                tabs: vec![],
                active: None
            }),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_burst_for_one_tab_is_one_reload() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        for i in 0..20 {
            r.changed("t1", Some("t1"), t0 + Duration::from_millis(i * 10));
            assert!(
                r.due(t0 + Duration::from_millis(i * 10 + 100)).is_empty(),
                "still gathering"
            );
            assert!(r.waiting());
        }
        // 310 ms after the last of them: not settled yet. Then one reload, and nothing after it.
        assert!(r.due(t0 + Duration::from_millis(500)).is_empty());
        assert_eq!(r.due(t0 + Duration::from_millis(900)), ["t1"]);
        r.reloaded("t1");
        assert!(r.due(t0 + Duration::from_secs(9)).is_empty());
        assert!(!r.waiting());
    }

    #[test]
    fn a_hidden_tab_is_marked_and_clears_when_it_is_shown() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("hidden", Some("shown"), t0);
        // Nothing to load for a hidden tab, and no timer to arm for it.
        assert!(r.due(t0 + SETTLE * 10).is_empty());
        assert!(!r.waiting());
        assert!(r.is_stale("hidden"), "say so on the tab");
        assert!(!r.is_stale("shown"), "the page being read is never marked");
        // Showing it is what refreshes it, and it is asked once.
        assert!(r.focus("hidden"));
        assert!(!r.focus("hidden"));
        assert!(!r.is_stale("hidden"));
        // Hidden again while it changed: marked again.
        r.changed("hidden", Some("shown"), t0);
        assert!(r.is_stale("hidden"));
    }

    #[test]
    fn showing_a_tab_that_is_about_to_reload_anyway_does_not_double_up() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        assert!(
            !r.focus("t1"),
            "it was never hidden, so nothing to catch up"
        );
        assert_eq!(r.due(t0 + SETTLE), ["t1"], "and it still reloads once");
        r.reloaded("t1");
        assert!(r.due(t0 + SETTLE * 4).is_empty());
    }

    #[test]
    fn a_reload_is_held_for_a_reader_mid_drag_but_not_for_ever() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        assert_eq!(r.due(t0 + SETTLE), ["t1"], "due, and asked");
        let mut held = 0;
        while r.may_hold("t1") {
            let now = t0 + SETTLE * (held as u32 + 1);
            r.held("t1", now);
            held += 1;
            assert!(
                r.due(now).is_empty(),
                "waiting for the reader another round"
            );
            assert_eq!(r.due(now + SETTLE), ["t1"], "and asked again after it");
        }
        assert_eq!(held, MAX_HELD, "a bounded number of rounds");
        assert!(!r.may_hold("t1"), "after that the page changes under them");
        // Not reloaded and never dropped: it stays due until the window reloads it.
        assert_eq!(r.due(t0 + SETTLE * 99), ["t1"]);
        r.reloaded("t1");
        assert!(r.due(t0 + SETTLE * 99).is_empty());
    }

    #[test]
    fn a_new_change_resets_both_the_wait_and_the_patience() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        for _ in 0..MAX_HELD {
            r.held("t1", t0);
        }
        assert!(!r.may_hold("t1"));
        r.changed("t1", Some("t1"), t0 + SETTLE);
        assert!(r.may_hold("t1"), "new news, fresh patience");
        assert!(
            r.due(t0 + SETTLE + SETTLE / 2).is_empty(),
            "and a fresh wait from the new change"
        );
        assert_eq!(r.due(t0 + SETTLE * 2), ["t1"]);
    }

    #[test]
    fn only_tabs_on_the_desk_keep_a_reload_or_a_mark() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        r.changed("t2", Some("t1"), t0);
        r.keep(&HashSet::from(["t1"]));
        assert!(!r.is_stale("t2"), "the hidden tab closed");
        assert_eq!(r.due(t0 + SETTLE * 2), ["t1"]);
        r.clear();
        assert!(!r.waiting());
        assert!(!r.is_stale("t1"));
        r.changed("t1", Some("t1"), t0);
        assert_eq!(r.due(t0 + SETTLE), ["t1"], "a new connection starts clean");
    }

    #[test]
    fn two_tabs_showing_the_same_page_both_catch_up() {
        let d = desk(
            &[
                ("t1", page("site://plans/x.md")),
                ("t2", page("site://plans/x")),
            ],
            Some("t1"),
        );
        let t0 = Instant::now();
        let mut r = Refresher::default();
        for id in page_tabs(&d, "site://plans/x.md") {
            r.changed(&id, Some("t1"), t0);
        }
        // The shown one is coalesced; its twin is marked and loads when shown.
        assert_eq!(r.due(t0 + SETTLE), ["t1"]);
        assert!(r.is_stale("t2"));
        assert!(r.focus("t2"));
    }
}
