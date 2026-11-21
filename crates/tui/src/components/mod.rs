//! UI components. Each renders from `AppState` and turns keys into actions.

mod approval_popup;
mod changes_view;
mod chat_lines;
mod chat_pane;
mod command_line;
mod component;
mod composer;
mod confirm_popup;
mod help;
mod page_viewer;
mod picker;
mod root;
mod selection;
mod status_bar;
mod tab_bar;
mod terminal_view;
mod widgets;

pub use approval_popup::ApprovalPopup;
pub use changes_view::ChangesView;
pub use chat_pane::ChatPane;
pub use command_line::CommandLine;
pub use component::Component;
pub use composer::Composer;
pub use confirm_popup::ConfirmPopup;
pub use help::Help;
pub use page_viewer::PageViewer;
pub use picker::Picker;
pub use root::Root;
pub use selection::Selection;
pub use status_bar::StatusBar;
pub use tab_bar::TabBar;
pub use terminal_view::TerminalView;
