//! A terminal emulator model without a UI (ported from the desktop's terminal pane): bytes from
//! a program go in, a grid of styled cells, a cursor and scrollback come out, and keys are
//! encoded the way the program asked for (the kitty keyboard protocol too). A UI paints the
//! cells and sends [`TermScreen::take_output`] to the program.

pub mod grid;
pub mod key_map;
mod output;
pub mod term_config;
mod term_screen;
mod types;

pub use output::{Output, Sink};
pub use tattoy_wezterm_term::{KeyCode, KeyModifiers};
pub use term_screen::TermScreen;
pub use types::{CellView, Paint};
