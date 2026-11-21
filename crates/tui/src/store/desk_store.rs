//! The session's desk, as this terminal shows it (plan phase 12, ticket 0208).
//!
//! The desk is the server's: the same one the desktop and the phone render, and the one the
//! agent's `show_page` lands on. The TUI reads it and draws the tabs a terminal can draw — a
//! page, a project's changes. `Terminal` and `LiveView` tabs are kept exactly as received: never
//! shown, never written back.

use composer_core::SiteUrl;
use composer_protocol::{Desk, DeskTab};

pub use crate::store::stage_tab::StageTab;

/// The id a page tab has on the desk. The server derives an id from what the tab shows, so the
/// same page is one tab however it was reached; this is the same derivation, so opening a page
/// here and there counts as one. `None` for a URL that names no page.
pub fn page_key(url: &str) -> Option<String> {
    let url = url.trim();
    let path = url.strip_prefix("site://").unwrap_or(url);
    let (site, rest) = path.split_once('/').unwrap_or((path, ""));
    let page = SiteUrl::new(site, rest).ok()?;
    Some(format!("page:{page}"))
}

/// The desk as the session's owner sent it, plus which of its tabs this terminal shows.
#[derive(Debug, Default)]
pub struct DeskStore {
    /// Received as-is: the tabs this terminal cannot draw stay in it untouched.
    pub desk: Desk,
    /// The tab on the screen. A tab id, or `None` for the empty stage.
    shown: Option<String>,
}

impl DeskStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// The tabs a terminal can draw, in the desk's order.
    pub fn visible(&self) -> Vec<StageTab> {
        self.desk
            .tabs
            .iter()
            .filter_map(|entry| match &entry.tab {
                DeskTab::Page { url } => Some(StageTab::Page {
                    id: entry.id.clone(),
                    url: url.clone(),
                    unseen: entry.unseen,
                }),
                DeskTab::Changes { site } => Some(StageTab::Changes {
                    id: entry.id.clone(),
                    site: site.clone(),
                    unseen: entry.unseen,
                }),
                // An app, a terminal, the live view: not this terminal's to show or change.
                _ => None,
            })
            .collect()
    }

    /// The tab on the screen, as this terminal decided (`None`: the empty stage). Not whether
    /// the desk says so: see [`AppState`](crate::store::AppState), which keeps the two together.
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    /// The desk as its owner sent it, and nothing else: which of its tabs to show is the
    /// client's decision (see `show` and `show_desk`), because it is the client that decides
    /// whether it follows the agent.
    pub fn set_desk(&mut self, desk: Desk) {
        self.desk = desk;
    }

    /// Whether `id` names a tab in this desk that a terminal can draw: asked of
    /// [`visible`], so the two can never disagree about what is drawable.
    pub fn can_draw(&self, id: &str) -> bool {
        self.visible().iter().any(|tab| tab.id() == id)
    }

    /// Forget this session's desk (attaching another one: its tabs are not ours).
    pub fn clear(&mut self) {
        self.desk = Desk::default();
        self.shown = None;
    }

    /// Shows `id`. An empty id means nothing is on the screen, which is what an empty desk
    /// leaves the stage with; it is not a tab called "".
    pub fn show(&mut self, id: &str) {
        self.shown = (!id.is_empty()).then(|| id.to_string());
    }

    /// Show what the desk shows, when that is one of `ids` (the tabs that exist); the last
    /// of `ids` otherwise, and nothing if there are none.
    pub fn show_desk(&mut self, ids: &[String]) {
        self.shown = self
            .desk
            .active
            .clone()
            .filter(|id| ids.iter().any(|tab| tab == id))
            .or_else(|| ids.last().cloned());
    }

    /// The shown tab has to be one that exists. Keep it when it is; otherwise show what the
    /// desk shows, and failing that the last tab there is (nothing, if none).
    pub fn reconcile(&mut self, ids: &[String]) {
        if let Some(shown) = &self.shown
            && ids.iter().any(|id| id == shown)
        {
            return;
        }
        self.show_desk(ids);
    }

    /// Point at the tab next to the one closing (the next, else the previous), as the desk
    /// does. Only when it was the one shown; an id that no longer exists is nothing to do.
    pub fn closing(&mut self, id: &str) {
        if self.shown.as_deref() != Some(id) {
            return;
        }
        let all: Vec<String> = self
            .visible()
            .iter()
            .map(|tab| tab.id().to_string())
            .collect();
        let Some(at) = all.iter().position(|tab| tab == id) else {
            return;
        };
        let rest: Vec<String> = all
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.as_str() != id)
            .map(|(_, tab)| tab.clone())
            .collect();
        self.shown = rest.get(at).or_else(|| rest.last()).cloned();
    }
}
