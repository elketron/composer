use composer_protocol::SessionInfo;
use composer_protocol::http::{SearchHit, SiteSummary};

use crate::effect::Effect;
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

    pub fn open(&mut self, kind: PickerKind, sessions: &[SessionInfo]) -> Vec<Effect> {
        self.kind = kind;
        self.query.clear();
        self.selected = 0;
        self.all.clear();
        self.items.clear();
        match kind {
            PickerKind::Pages => vec![Effect::Search(String::new())],
            PickerKind::Sites => vec![Effect::LoadSites],
            PickerKind::Sessions => {
                self.all = sessions.iter().map(session_item).collect();
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
                if s.has_ui {
                    right.push("app");
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

fn session_item(s: &SessionInfo) -> PickerItem {
    let indent = if s.parent.is_some() { "  " } else { "" };
    PickerItem {
        title: format!("{indent}{} #{}", s.agent, s.id),
        detail: s.preview.clone().unwrap_or_default(),
        right: match s.state {
            composer_protocol::TurnState::Idle => {
                s.updated.get(..16).unwrap_or("").replace('T', " ")
            }
            state => format!("{state:?}").to_lowercase(),
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
