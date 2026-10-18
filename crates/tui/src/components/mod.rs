//! UI components. Each renders from `AppState` and turns keys into actions.

mod approval_popup;
mod chat_lines;
mod chat_pane;
mod command_line;
mod component;
mod composer;
mod page_viewer;
mod picker;
mod root;
mod status_bar;
mod widgets;

pub use approval_popup::ApprovalPopup;
pub use chat_pane::ChatPane;
pub use command_line::CommandLine;
pub use component::Component;
pub use composer::Composer;
pub use page_viewer::PageViewer;
pub use picker::Picker;
pub use root::Root;
pub use status_bar::StatusBar;
