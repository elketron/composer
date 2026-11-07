//! A session's desk (plan phase 5): the tabs on the right of its chat, which come back with it.

use serde::{Deserialize, Serialize};

/// What a tab shows. Its id is derived from this, so the same thing is never open twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeskTab {
    /// An app: on a project (by name), or on Home (`None`).
    App {
        site: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// A page (`site://...`).
    Page { url: String },
    /// A server-owned terminal.
    Terminal { id: String },
    /// A project's changes (its ticket branches, its diff).
    Changes { project: String },
}

/// A tab on a desk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DeskEntry {
    /// `app:cad@game`, `app:calendar`, `page:site://x`, `terminal:<id>`, `changes:<project>`.
    pub id: String,
    pub tab: DeskTab,
    /// Opened by the agent and not looked at yet (a badge, for a client not following it).
    #[serde(default)]
    pub unseen: bool,
}

/// A session's desk: its tabs in order, and the one shown. Empty: the project home.
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
