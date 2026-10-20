//! Taking over the terminal and giving it back.

use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::DefaultTerminal;

/// Raw mode, alternate screen, and bracketed paste (a pasted path arrives in one piece, so it
/// can become an attachment).
pub fn init() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableBracketedPaste);
    terminal
}

pub fn restore() {
    let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
    ratatui::restore();
}
