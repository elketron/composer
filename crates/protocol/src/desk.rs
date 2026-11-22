//! A session's desk (plan phase 5): the tabs on the right of its chat, which come back with it.

use serde::{Deserialize, Serialize};

/// What a tab shows. Its id is derived from this, so the same thing is never open twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeskTab {
    /// A page (`site://...`).
    Page { url: String },
    /// A server-owned terminal.
    Terminal { id: String },
    /// The changes (git diff) of a site's `folder:`.
    Changes { site: String },
    /// The live view of the user's application (wayai's viewer, proxied at `/view`).
    /// It names no site; one per desk (id `live`). Wire form: `{"kind":"live_view"}`.
    LiveView,
    /// One of the user's own local web servers (`[apps.<name>]`, served at `/apps/<name>/`), at
    /// `path` relative to its root (`""`: its page; may carry `?query`). One per app per desk
    /// (id `app:<name>`): opening it at another path moves that tab there.
    App {
        name: String,
        #[serde(default)]
        path: String,
    },
}

/// A tab on a desk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DeskEntry {
    /// `page:site://x`, `terminal:<id>`, `changes:<site>`, `live`, `app:<name>`.
    pub id: String,
    pub tab: DeskTab,
    /// Opened by the agent and not looked at yet (a badge, for a client not following it).
    #[serde(default)]
    pub unseen: bool,
}

impl DeskEntry {
    /// The start of a page tab's id: `page:` and then its normalised page URL. The server names page
    /// tabs with it (`SessionDesk::key`) and clients read the URL back with [`DeskEntry::page_url`],
    /// so the grammar has one owner.
    pub const PAGE_ID: &'static str = "page:";

    /// The normalised URL a page tab shows, read from its id; None for any other tab, or a page tab
    /// whose id does not have the page form (a desk from an older server).
    pub fn page_url(&self) -> Option<&str> {
        match self.tab {
            DeskTab::Page { .. } => self.id.strip_prefix(Self::PAGE_ID),
            _ => None,
        }
    }
}

/// A session's desk: its tabs in order, and the one shown. Empty: the session's home.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Desk {
    #[serde(default)]
    pub tabs: Vec<DeskEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
}

/// Who changed a desk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeskBy {
    User,
    Agent,
}
