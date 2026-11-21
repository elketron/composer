use composer_protocol::SessionInfo;
use composer_protocol::http::{SearchHit, SiteSummary};

use crate::commands::kind_name;
use crate::effect::Effect;
use crate::store::SessionStore;
use crate::store::types::{PickerItem, PickerKind, PickerTarget};

/// The floating picker (`Space p/s/w`).
#[derive(Debug)]
pub struct PickerStore {
    pub kind: PickerKind,
    pub query: String,
    pub items: Vec<PickerItem>,
    pub selected: usize,
    /// All candidates for client-side filtering (sites, sessions).
    all: Vec<PickerItem>,
}

impl PickerStore {
    pub fn new() -> Self {
        Self {
            kind: PickerKind::Pages,
            query: String::new(),
            items: Vec::new(),
            selected: 0,
            all: Vec::new(),
        }
    }

    pub fn open(&mut self, kind: PickerKind, store: &SessionStore) -> Vec<Effect> {
        self.kind = kind;
        self.query.clear();
        self.selected = 0;
        self.all.clear();
        self.items.clear();
        match kind {
            PickerKind::Pages => vec![Effect::Search(String::new())],
            PickerKind::Sites => vec![Effect::LoadSites],
            PickerKind::Sessions => {
                // A tree: each top-level session (most recent first), then the ones it spawned.
                let sessions = &store.sessions;
                // `:scope` narrows it to one site.
                let top = sessions
                    .iter()
                    .filter(|s| s.parent.is_none() && store.in_scope(s));
                self.all = top
                    .flat_map(|parent| {
                        let children = sessions
                            .iter()
                            .filter(|s| s.parent.as_deref() == Some(&parent.id));
                        std::iter::once(parent).chain(children)
                    })
                    .map(|s| session_item(s, store))
                    .collect();
                self.filter();
                vec![]
            }
        }
    }

    pub fn set_query(&mut self, query: String) -> Vec<Effect> {
        self.query = query;
        self.selected = 0;
        match self.kind {
            PickerKind::Pages => vec![Effect::Search(self.query.clone())],
            PickerKind::Sites | PickerKind::Sessions => {
                self.filter();
                vec![]
            }
        }
    }

    /// Search results; stale ones (for an older query) are dropped.
    pub fn search_results(&mut self, query: &str, hits: Vec<SearchHit>) {
        if self.kind != PickerKind::Pages || query != self.query {
            return;
        }
        self.items = hits
            .into_iter()
            .map(|h| PickerItem {
                detail: h.url.trim_start_matches("site://").to_string(),
                title: h.title,
                right: h.site,
                target: PickerTarget::Url(h.url),
            })
            .collect();
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
    }

    pub fn sites(&mut self, sites: Vec<SiteSummary>) {
        if self.kind != PickerKind::Sites {
            return;
        }
        self.all = sites
            .into_iter()
            .map(|s| {
                let mut right = Vec::new();
                if s.has_justfile {
                    right.push("actions");
                }
                PickerItem {
                    detail: format!("site://{}", s.name),
                    target: PickerTarget::Url(format!("site://{}", s.name)),
                    title: s.title,
                    right: right.join(" · "),
                }
            })
            .collect();
        self.filter();
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(len) as usize;
    }

    pub fn selected_target(&self) -> Option<PickerTarget> {
        self.items.get(self.selected).map(|i| i.target.clone())
    }

    fn filter(&mut self) {
        let query = self.query.to_lowercase();
        self.items = self
            .all
            .iter()
            .filter(|i| fuzzy(&query, &format!("{} {}", i.title, i.detail).to_lowercase()))
            .cloned()
            .collect();
    }
}

fn session_item(s: &SessionInfo, store: &SessionStore) -> PickerItem {
    let indent = if s.parent.is_some() { "  " } else { "" };
    let here = if store.current.as_deref() == Some(&s.id) {
        " ◂"
    } else {
        ""
    };
    let preview = s.preview.clone().unwrap_or_default();
    PickerItem {
        title: format!("{indent}{} #{}{here}", kind_name(s.kind), s.id),
        detail: preview,
        right: match store.status(s) {
            "idle" => s.updated.get(..16).unwrap_or("").replace('T', " "),
            busy => format!("● {busy}"),
        },
        target: PickerTarget::Session(s.id.clone()),
    }
}

/// Every query character appears in order.
fn fuzzy(query: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    query
        .chars()
        .filter(|c| !c.is_whitespace())
        .all(|q| chars.any(|h| h == q))
}
