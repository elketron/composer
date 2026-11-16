//! The page on the stage, and the pages you have looked at (ticket 0208).
//!
//! Which page is on screen is the desk's shown tab saying so (see
//! [`DeskStore`](crate::store::DeskStore)); this holds the content of that tab. Pages are kept
//! per tab id, so returning to a tab is instant and a fetch that finishes late cannot drop a
//! different page into it.

use std::collections::HashMap;

use crate::effect::Effect;
use crate::store::desk_store::page_key;
use crate::store::types::PageView;

/// What a page tab is made of: what it was asked to show, and what arrived.
#[derive(Debug, Default)]
struct Slot {
    /// The URL as it was asked for, fragment and all: where `H` / `L` go back to.
    url: Option<String>,
    page: Option<PageView>,
}

/// The page viewer: the shown page, a cache by tab, back and forward, follow-agent.
#[derive(Debug)]
pub struct PageStore {
    pub current: Option<PageView>,
    /// The URL being fetched, for the pane's `…` title.
    pub loading: Option<String>,
    pub error: Option<String>,
    /// Move the stage to the page the agent is on, until you navigate yourself.
    pub follow: bool,
    /// By tab id (`page:site://…`).
    slots: HashMap<String, Slot>,
    /// The tab whose page is on screen, kept in step by
    /// [`AppState::sync_stage`](crate::store::AppState::sync_stage).
    showing: Option<String>,
    /// Tabs you left, most recent last (`H`), and the ones you came back from (`L`).
    back: Vec<String>,
    forward: Vec<String>,
}

impl Default for PageStore {
    fn default() -> Self {
        // Following the agent's pages is what makes the stage useful before you touch anything.
        Self::new()
    }
}

impl PageStore {
    pub fn new() -> Self {
        Self {
            current: None,
            loading: None,
            error: None,
            follow: true,
            slots: HashMap::new(),
            showing: None,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    /// Show the page of tab `id`, asked for as `url`. Called whenever the shown tab changes,
    /// and does nothing when its page is already here.
    pub fn show(&mut self, id: &str, url: &str) -> Option<Effect> {
        self.remember(id, url);
        self.goto(id, url, false)
    }

    /// The page you asked to see. Its tab id is the page, so this opens a tab; a page you have
    /// looked at before returns to its tab. Going forward from a page keeps the place you read
    /// it at (`L` still leads back), unless the link asked for another part of it.
    pub fn navigate(&mut self, url: &str) -> Option<Effect> {
        let mut url = url.to_string();
        let id = page_key(&url);
        if !url.contains('#')
            && let Some(asked) = self
                .back
                .iter()
                .rev()
                .chain(self.forward.iter().rev())
                .find_map(|tab| self.slots.get(tab))
                .and_then(|slot| slot.url.as_ref())
                .filter(|seen| same_page(seen, &url))
            && asked.contains('#')
        {
            url = asked.clone();
        }
        let here = self.showing.clone();
        let known = id.as_deref().is_some_and(|id| self.slots.contains_key(id));
        // A page with no tab id of its own (not a `site://` url) goes in the tab shown.
        let target = id.clone().or_else(|| here.clone()).unwrap_or_default();
        if Some(target.as_str()) != here.as_deref() {
            if let Some(prev) = here.filter(|prev| !prev.is_empty()) {
                self.back.push(prev);
                self.forward.clear();
            }
            if known {
                self.back.retain(|tab| tab != &target);
                self.forward.retain(|tab| tab != &target);
            }
        }
        if id.is_none() {
            self.remember(&target, &url);
        }
        self.goto(&target, &url, false)
    }

    /// Fetch this tab's page again (`r`). A page that failed has nothing on screen, which is
    /// exactly when you reach for `r`, so it asks for what the tab was opened with.
    pub fn reload(&mut self) -> Option<Effect> {
        let url = match &self.current {
            Some(page) => page.url.clone(),
            None => self.slots.get(self.showing.as_deref()?)?.url.clone()?,
        };
        Some(self.load(url))
    }

    /// Back to the tab you left before this one (`H`). Its id and URL, for whoever keeps the
    /// tabs in step.
    pub fn back(&mut self) -> Option<(String, String, Option<Effect>)> {
        self.step(true)
    }

    /// Forward again (`L`).
    pub fn forward(&mut self) -> Option<(String, String, Option<Effect>)> {
        self.step(false)
    }

    /// The page that came back for tab `id`.
    pub fn loaded(&mut self, id: &str, page: PageView) {
        let slot = self.slots.entry(id.to_string()).or_default();
        slot.url = Some(page.url.clone());
        slot.page = Some(page.clone());
        if self.showing.as_deref() == Some(id) {
            self.current = Some(page);
            self.loading = None;
            self.error = None;
        }
    }

    pub fn failed(&mut self, id: &str, url: &str, error: String) {
        if self.showing.as_deref() != Some(id) {
            return;
        }
        if self.loading.as_deref() == Some(url) {
            self.loading = None;
        }
        self.error = Some(format!("{url}: {error}"));
    }

    /// A page changed on the server, so this cache is stale.
    pub fn invalidate(&mut self, url: &str) {
        let Some(id) = page_key(url) else {
            return;
        };
        if let Some(slot) = self.slots.get_mut(&id) {
            slot.page = None;
        }
        if self.showing.as_deref() == Some(id.as_str()) {
            self.current = None;
        }
    }

    /// The tabs this desk no longer has: forget them. A session's pages are not the next
    /// session's pages.
    pub fn forget(&mut self, ids: &[String]) {
        self.slots.retain(|id, _| ids.iter().any(|keep| keep == id));
        self.back.retain(|id| ids.iter().any(|keep| keep == id));
        self.forward.retain(|id| ids.iter().any(|keep| keep == id));
        if !self.showing.iter().any(|id| ids.contains(id)) {
            self.hide();
        }
    }

    /// Nothing of ours on screen (another kind of tab is shown, or none).
    pub fn hide(&mut self) {
        self.showing = None;
        self.current = None;
        self.loading = None;
        self.error = None;
    }

    /// Whether the tab on the screen is still waiting for its page: being fetched, or failed
    /// with the failure on show. Either way the next keypress must not ask for it again — a
    /// page that cannot be loaded would otherwise be fetched once per key, forever — while `r`
    /// and re-showing the tab both do.
    pub fn waiting_on_shown(&self) -> Option<&str> {
        self.showing
            .as_deref()
            .filter(|_| self.loading.is_some() || self.error.is_some())
    }

    /// Make tab `id`'s page the one on screen: from the cache when it's there, else fetched.
    /// With `keep`, a page that was never fetched leaves the screen as it is (`H` / `L`).
    fn goto(&mut self, id: &str, url: &str, keep: bool) -> Option<Effect> {
        // Already on its way to this exact tab: opening the page again changes nothing.
        if self.showing.as_deref() == Some(id) && self.loading.as_deref() == Some(url) {
            return None;
        }
        self.showing = Some(id.to_string());
        self.error = None;
        let cached = self
            .slots
            .get(id)
            .and_then(|slot| slot.page.clone())
            .filter(|page| same_page(&page.url, url));
        if let Some(page) = cached {
            self.current = Some(page);
            self.loading = None;
            return None;
        }
        if keep {
            return None;
        }
        self.current = None;
        Some(self.load(url.to_string()))
    }

    /// Walk one step: the tab at the top of the stack you asked for becomes the shown one and
    /// where you were goes onto the other, so the way back is never lost.
    fn step(&mut self, back: bool) -> Option<(String, String, Option<Effect>)> {
        let target = if back {
            self.back.pop()
        } else {
            self.forward.pop()
        }?;
        let url = self.slots.get(&target)?.url.clone()?;
        if let Some(here) = self.showing.clone().filter(|here| !here.is_empty()) {
            if back {
                self.forward.push(here);
            } else {
                self.back.push(here);
            }
        }
        let effect = self.goto(&target, &url, true);
        Some((target, url, effect))
    }

    /// What a page with no tab id of its own should be remembered as.
    fn remember(&mut self, id: &str, url: &str) {
        let slot = self.slots.entry(id.to_string()).or_default();
        slot.url = Some(url.to_string());
    }

    fn load(&mut self, url: String) -> Effect {
        self.loading = Some(url.clone());
        Effect::FetchPage(url)
    }
}

/// The same page however it was written: the tab is decided by `page_key`, so a fragment or a
/// trailing `.md` is the same page.
fn same_page(a: &str, b: &str) -> bool {
    let key = |u: &str| {
        let u = u.split('#').next().unwrap_or(u);
        page_key(u).unwrap_or_else(|| u.to_string())
    };
    key(a) == key(b)
}
