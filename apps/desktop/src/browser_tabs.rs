use std::collections::{HashMap, HashSet};

/// The stage's browser panes by tab id, and the tabs whose pane is on its way (an app's URL is
/// asked for first). Generic over the pane so it can be tested without a browser.
pub struct BrowserTabs<P> {
    panes: HashMap<String, P>,
    loading: HashSet<String>,
}

impl<P> Default for BrowserTabs<P> {
    fn default() -> Self {
        Self {
            panes: HashMap::new(),
            loading: HashSet::new(),
        }
    }
}

impl<P> BrowserTabs<P> {
    /// Whether `tab` needs a pane made now (none, and none on its way); marks it on its way.
    pub fn begin(&mut self, tab: &str) -> bool {
        !self.panes.contains_key(tab) && self.loading.insert(tab.to_string())
    }

    /// Its pane didn't come (the app couldn't open): it may be tried again.
    pub fn failed(&mut self, tab: &str) {
        self.loading.remove(tab);
    }

    pub fn arrived(&mut self, tab: String, pane: P) {
        self.loading.remove(&tab);
        self.panes.insert(tab, pane);
    }

    pub fn get(&self, tab: &str) -> Option<&P> {
        self.panes.get(tab)
    }

    /// Only the shown browser may paint, and not while its window is hidden. Focus is
    /// deliberately not an input: an unfocused window can still be visible.
    pub fn visibility<'a>(
        &'a self,
        shown: Option<&'a str>,
        window_visible: bool,
    ) -> impl Iterator<Item = (&'a P, bool)> {
        self.panes
            .iter()
            .map(move |(id, pane)| (pane, window_visible && shown == Some(id.as_str())))
    }

    /// The panes of the tabs not shown: none of them may keep the keyboard.
    pub fn hidden<'a>(&'a self, shown: Option<&'a str>) -> impl Iterator<Item = &'a P> {
        self.panes
            .iter()
            .filter(move |(id, _)| shown != Some(id.as_str()))
            .map(|(_, pane)| pane)
    }

    /// Only the tabs still on the desk keep their panes.
    pub fn keep(&mut self, tabs: &HashSet<&str>) {
        self.panes.retain(|id, _| tabs.contains(id.as_str()));
        self.loading.retain(|id| tabs.contains(id.as_str()));
    }

    /// All of them, made again when shown (a new connection: apps need new tokens).
    pub fn clear(&mut self) {
        self.panes.clear();
        self.loading.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_once_retried_after_failure_dropped_with_the_tab() {
        let mut tabs = BrowserTabs::<u8>::default();
        assert!(tabs.begin("a"));
        assert!(!tabs.begin("a"), "already on its way");
        tabs.failed("a");
        assert!(tabs.begin("a"), "tried again");
        tabs.arrived("a".into(), 1);
        assert!(!tabs.begin("a"), "has one");
        assert_eq!(tabs.get("a"), Some(&1));
        tabs.keep(&HashSet::from(["b"]));
        assert_eq!(tabs.get("a"), None);
        assert!(tabs.begin("a"));
        tabs.clear();
        assert!(tabs.begin("a"));
        // A tab that left the desk while its pane was on its way: made afresh when it's back.
        assert!(tabs.begin("c"));
        tabs.keep(&HashSet::new());
        assert!(tabs.begin("c"), "still marked as on its way");
    }

    #[test]
    fn only_the_shown_tab_in_a_visible_window_paints() {
        let mut tabs = BrowserTabs::default();
        tabs.arrived("animation".into(), 1);
        tabs.arrived("other".into(), 2);
        let states = |shown, visible| {
            let mut states: Vec<_> = tabs
                .visibility(shown, visible)
                .map(|(&pane, on)| (pane, on))
                .collect();
            states.sort();
            states
        };
        assert_eq!(states(Some("animation"), true), [(1, true), (2, false)]);
        assert_eq!(states(Some("other"), true), [(1, false), (2, true)]);
        assert_eq!(states(Some("animation"), false), [(1, false), (2, false)]);
        assert_eq!(states(Some("terminal"), true), [(1, false), (2, false)]);
        assert_eq!(states(None, true), [(1, false), (2, false)]);
    }

    #[test]
    fn hidden_is_every_pane_but_the_shown_one() {
        let mut tabs = BrowserTabs::default();
        tabs.arrived("app".into(), 1);
        tabs.arrived("page".into(), 2);
        let hidden = |shown| {
            let mut panes: Vec<_> = tabs.hidden(shown).copied().collect();
            panes.sort();
            panes
        };
        assert_eq!(hidden(Some("app")), [2]);
        assert_eq!(hidden(Some("page")), [1]);
        assert_eq!(hidden(Some("terminal")), [1, 2]);
        assert_eq!(hidden(None), [1, 2]);
    }
}
