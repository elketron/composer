//! Flux stores: all state shared between components lives here.

mod app_state;
mod approval_store;
mod attachment_store;
mod changes_store;
mod connection_store;
mod desk_store;
mod page_slot;
mod page_store;
mod picker_store;
mod session_store;
mod stage_tab;
mod terminal_store;
pub mod types;
mod ui_store;

pub use app_state::AppState;
pub use approval_store::ApprovalStore;
pub use attachment_store::AttachmentStore;
pub use changes_store::ChangesStore;
pub use connection_store::ConnectionStore;
pub use desk_store::{DeskStore, StageTab, page_key};
pub use page_store::PageStore;
pub use picker_store::PickerStore;
pub use session_store::SessionStore;
pub use terminal_store::{TerminalSink, TerminalStore};
pub use ui_store::UiStore;
