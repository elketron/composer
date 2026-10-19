use composer_protocol::SessionInfo;
use composer_protocol::http::{SearchHit, SiteSummary};

use crate::effect::Effect;
use crate::store::SessionStore;
use crate::store::types::{PickerItem, PickerKind, PickerTarget};

/// The floating picker (`Space p/s/w/o`).
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
                // A tree: each top-level session (most recent first), then its workers.
                let sessions = &store.sessions;
                let top = sessions.iter().filter(|s| s.parent.is_none());
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
            PickerKind::Orchestration => {
                let members = store.orchestration();
                self.all = members.iter().map(|s| session_item(s, store)).collect();
                self.filter();
                self.selected = switch_target(&members, store.current.as_deref());
                vec![]
            }
        }
    }

    pub fn set_query(&mut self, query: String) -> Vec<Effect> {
        self.query = query;
        self.selected = 0;
        match self.kind {
            PickerKind::Pages => vec![Effect::Search(self.query.clone())],
            PickerKind::Sites | PickerKind::Sessions | PickerKind::Orchestration => {
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
                if s.app_built {
                    right.push("app");
                } else if s.has_ui {
                    right.push("app (not built)");
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
        title: format!("{indent}{} #{}{here}", s.agent, s.id),
        detail: match &s.ticket {
            Some(ticket) if !preview.starts_with("ticket ") => {
                format!("ticket {ticket} · {preview}")
            }
            _ => preview,
        },
        right: match store.status(s) {
            "idle" => s.updated.get(..16).unwrap_or("").replace('T', " "),
            busy @ ("running" | "waiting" | "approval" | "starting") => format!("● {busy}"),
            finished => finished.to_string(),
        },
        target: PickerTarget::Session(s.id.clone()),
    }
}

/// Where the orchestration picker starts: from the orchestrator, the first busy worker (else
/// the first worker); from a worker, the orchestrator. Enter then switches right away.
fn switch_target(members: &[&SessionInfo], current: Option<&str>) -> usize {
    match members.iter().position(|s| Some(s.id.as_str()) == current) {
        Some(0) => members
            .iter()
            .skip(1)
            .position(|s| s.state != composer_protocol::TurnState::Idle)
            .map_or(1, |i| i + 1),
        _ => 0,
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
