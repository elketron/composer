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
    }
}
