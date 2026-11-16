//! One page tab's content in the [`PageStore`](crate::store::PageStore)'s cache (ticket 0208).

use crate::store::types::PageView;

/// What a page tab is made of: what it was asked to show, and what arrived.
#[derive(Debug, Default)]
pub(crate) struct Slot {
    /// The URL as it was asked for, fragment and all: where `H` / `L` go back to.
    pub(crate) url: Option<String>,
    pub(crate) page: Option<PageView>,
}
