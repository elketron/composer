//! When a stage tab shows its page or diff again, once [`crate::page_refresh`] has said which tabs
//! a change is about.
//!
//! Three rules, and this type is the only place they are decided:
//!
//! - **A burst is one reload.** The tab on screen waits [`SETTLE`] after the *last* change, so
//!   twenty events for one page become one load. (The server's filewatchers settle and debounce for
//!   the same reason, ticket 0119.)
//! - **A tab nobody is looking at does nothing.** Its pane is left alone and marked; the load
//!   happens when it is shown. Reloading a hidden pane costs a browser painting for something no one
//!   sees, and hidden panes are what this app has had to be careful with.
//! - **A reader is not interrupted, only for a while.** A reload waits for someone mid-drag or
//!   mid-selection in the page, up to `MAX_HELD` rounds, then happens anyway.
//!
//! Nothing is ever dropped: a pending tab stays pending until [`Refresher::take_due`] hands it back,
//! a tab that left the desk is forgotten by [`Refresher::keep`] because there is nothing left to
//! show, and a tab that is no longer shown when its wait passes becomes a mark instead.
//!
//! It knows tab ids only — what a tab shows and whether it has a pane is the window's business — so
//! none of this needs a browser.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How long a tab waits after a change before it reloads, and how long a held reload waits again.
pub const SETTLE: Duration = Duration::from_millis(400);

/// How many of those waits a reload may be held back for a reader mid-interaction, before it is
/// applied anyway. Four is just under two seconds: long enough to finish selecting a line, short
/// enough that a change that arrived still looks like it arrived. `max_hold_total` pins it, so it
/// cannot quietly grow into a page that appears to stop updating.
const MAX_HELD: u8 = 4;

/// What a tab is waiting for.
struct Pending {
    /// The earliest it reloads.
    at: Instant,
    /// How often the reload was held back for the reader.
    held: u8,
}

/// Which tabs need a reload and when: the coalescing window for the tab on screen, and the mark a
/// hidden tab carries until it is shown.
#[derive(Default)]
pub struct Refresher {
    pending: HashMap<String, Pending>,
    /// Tabs whose content changed while they were hidden.
    marked: HashSet<String>,
}

impl Refresher {
    /// Tab `tab` shows something that just changed. The tab on screen waits [`SETTLE`] first, so a
    /// burst of events for one page is one reload; any other is only marked, and loads when it is
    /// shown. A new change starts the wait and the patience again: the reader is about to see newer
    /// news, and one more wait is cheaper than two loads.
    pub fn changed(&mut self, tab: &str, shown: Option<&str>, now: Instant) {
        if Some(tab) == shown {
            self.marked.remove(tab);
            self.pending.insert(
                tab.to_string(),
                Pending {
                    at: now + SETTLE,
                    held: 0,
                },
            );
        } else {
            self.marked.insert(tab.to_string());
        }
    }

    /// Whether anything is waiting: the window arms its timer only while this holds.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Take every tab whose wait has passed, in a stable order, and decide each one:
    ///
    /// - **reload it**: it is the shown tab and the reader is not mid-interaction in it — these are
    ///   what the call returns;
    /// - **wait one more round** for the reader, while `MAX_HELD` allows, then reload regardless;
    /// - **stop waiting** if it is not the shown tab any more: it is marked instead, and loads when
    ///   it is shown again, like any tab that was hidden when its page changed.
    ///
    /// `interacting` says whether the reader holds a mouse button or a selection in that tab's pane.
    /// A held-back tab stays pending, so the next timer round asks again.
    pub fn take_due(
        &mut self,
        now: Instant,
        shown: Option<&str>,
        interacting: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        let mut due: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, wait)| wait.at <= now)
            .map(|(id, _)| id.clone())
            .collect();
        due.sort_unstable();
        let mut reload = Vec::new();
        for id in due {
            if Some(id.as_str()) != shown {
                // Switched away while it waited: no browser work for a tab nobody is looking at.
                self.pending.remove(&id);
                self.marked.insert(id);
                continue;
            }
            let held = {
                let Some(wait) = self.pending.get_mut(&id) else {
                    continue;
                };
                if interacting(&id) && wait.held < MAX_HELD {
                    wait.at = now + SETTLE;
                    wait.held += 1;
                    true
                } else {
                    false
                }
            };
            if held {
                continue;
            }
            self.pending.remove(&id);
            self.marked.remove(&id);
            reload.push(id);
        }
        reload
    }

    /// `tab` is the shown one now: whether it must load, because its page changed while it was
    /// hidden or a reload for it was still waiting. Either is consumed here, so a tab is never
    /// asked twice for the same news.
    pub fn focus(&mut self, tab: &str) -> bool {
        self.pending.remove(tab).is_some() || self.marked.remove(tab)
    }

    /// Whether to hint on the tab that its text is older than the page. Only tabs that are not
    /// shown are ever marked, so this never appears on the page someone is reading.
    pub fn is_stale(&self, tab: &str) -> bool {
        self.marked.contains(tab)
    }

    /// Only tabs still on the desk keep a reload or a mark.
    pub fn keep(&mut self, tabs: &HashSet<&str>) {
        self.pending.retain(|id, _| tabs.contains(id.as_str()));
        self.marked.retain(|id| tabs.contains(id.as_str()));
    }

    /// A new connection: nothing on screen is known to be current or not (and every pane is being
    /// made again anyway, which loads its page as it is).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.marked.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reader is not mid-interaction: the plain case, a page changing while it is read.
    fn idle(_: &str) -> bool {
        false
    }

    /// The reader holds a mouse button down in every pane it is asked about.
    fn dragging(_: &str) -> bool {
        true
    }

    #[test]
    fn a_burst_for_one_tab_is_one_reload() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        for i in 0..20u32 {
            r.changed("t1", Some("t1"), t0 + Duration::from_millis(i as u64 * 10));
            assert!(
                r.take_due(
                    t0 + Duration::from_millis(i as u64 * 10 + 100),
                    Some("t1"),
                    idle
                )
                .is_empty(),
                "still gathering"
            );
            assert!(r.waiting());
        }
        // 310 ms after the last of them: not settled yet. Then one reload, and nothing after it.
        assert!(
            r.take_due(t0 + Duration::from_millis(500), Some("t1"), idle)
                .is_empty()
        );
        assert_eq!(
            r.take_due(t0 + Duration::from_millis(900), Some("t1"), idle),
            ["t1"]
        );
        assert!(!r.waiting(), "the entry is taken, not left waiting");
        assert!(
            r.take_due(t0 + Duration::from_secs(9), Some("t1"), idle)
                .is_empty()
        );
    }

    #[test]
    fn the_wait_is_counted_from_the_last_change() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        r.changed("t1", Some("t1"), t0 + SETTLE / 2);
        assert!(
            r.take_due(t0 + SETTLE, Some("t1"), idle).is_empty(),
            "the second change moved the deadline"
        );
        assert_eq!(r.take_due(t0 + SETTLE * 2, Some("t1"), idle), ["t1"]);
    }

    #[test]
    fn a_hidden_tab_is_marked_and_clears_when_it_is_shown() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("hidden", Some("shown"), t0);
        // Nothing to load for a hidden tab, and no timer to arm for it.
        assert!(r.take_due(t0 + SETTLE * 10, Some("shown"), idle).is_empty());
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
    fn switching_away_within_the_wait_defers_the_load() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        assert!(!r.focus("t2"), "the other tab had nothing waiting");
        // Its wait passes while t2 is the shown tab: no browser work for a tab nobody sees.
        assert!(r.take_due(t0 + SETTLE, Some("t2"), idle).is_empty());
        assert!(r.is_stale("t1"));
        assert!(!r.waiting(), "and no timer keeps asking for it");
        assert!(r.focus("t1"), "showing it is what loads it");
        assert!(!r.is_stale("t1"));
    }

    #[test]
    fn a_reload_is_held_for_a_reader_mid_drag_and_then_happens() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        for round in 1..=MAX_HELD as u32 {
            assert!(
                r.take_due(t0 + SETTLE * round, Some("t1"), dragging)
                    .is_empty(),
                "round {round}: still waiting for the reader"
            );
            assert!(r.waiting(), "a held reload is never dropped");
        }
        // One more round and the page changes under them.
        assert_eq!(
            r.take_due(t0 + SETTLE * (MAX_HELD as u32 + 1), Some("t1"), dragging),
            ["t1"],
            "the bound is honoured, then the news arrives"
        );
        assert!(!r.waiting());
        assert!(!r.is_stale("t1"), "and it is not left marked");
    }

    /// The bound as a duration, not just a count: the longest a reader mid-drag may wait for a page
    /// that changed. Pins `MAX_HELD` and `SETTLE` together.
    #[test]
    fn max_hold_total() {
        let longest = SETTLE * (MAX_HELD as u32 + 1);
        assert!(
            longest <= Duration::from_millis(2500),
            "a reload may be held for {} ms, which is too long to be called a wait",
            longest.as_millis()
        );
    }

    #[test]
    fn a_new_change_buys_a_fresh_wait_and_a_fresh_patience() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        for round in 1..=MAX_HELD as u32 {
            assert!(
                r.take_due(t0 + SETTLE * round, Some("t1"), dragging)
                    .is_empty()
            );
        }
        // Out of patience — but the next round brings newer news instead of being asked to wait.
        let newer = t0 + SETTLE * (MAX_HELD as u32 + 1);
        r.changed("t1", Some("t1"), newer);
        for round in 1..=MAX_HELD as u32 {
            assert!(
                r.take_due(newer + SETTLE * round, Some("t1"), dragging)
                    .is_empty(),
                "a new change is not due at once, and is held again"
            );
        }
        assert_eq!(
            r.take_due(newer + SETTLE * (MAX_HELD as u32 + 1), Some("t1"), dragging),
            ["t1"]
        );
    }

    #[test]
    fn a_tab_that_left_the_desk_or_the_connection_is_forgotten() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        r.changed("t1", Some("t1"), t0);
        r.changed("t2", Some("t1"), t0);
        r.keep(&HashSet::from(["t1"]));
        assert!(!r.is_stale("t2"), "the hidden tab closed");
        assert_eq!(r.take_due(t0 + SETTLE * 2, Some("t1"), idle), ["t1"]);
        r.changed("t1", Some("t1"), t0);
        r.clear();
        assert!(!r.waiting(), "a reconnect leaves nothing pending");
        assert!(!r.is_stale("t1"));
        assert!(
            r.take_due(t0 + SETTLE * 9, Some("t1"), idle).is_empty(),
            "and starts clean"
        );
        r.changed("t1", Some("t1"), t0);
        assert_eq!(r.take_due(t0 + SETTLE, Some("t1"), idle), ["t1"]);
    }

    #[test]
    fn two_tabs_on_one_page_do_not_both_load() {
        let t0 = Instant::now();
        let mut r = Refresher::default();
        // `page_refresh` returns every tab showing the page; the window routes them here one by one.
        for id in ["page:site://plans/x.md", "page:site://plans/x"] {
            r.changed(id, Some("page:site://plans/x.md"), t0);
        }
        assert_eq!(
            r.take_due(t0 + SETTLE, Some("page:site://plans/x.md"), idle),
            ["page:site://plans/x.md"]
        );
        let hidden = "page:site://plans/x";
        assert!(r.is_stale(hidden));
        assert!(r.focus(hidden), "and it loads the moment it is shown");
        assert!(!r.is_stale(hidden));
    }
}
