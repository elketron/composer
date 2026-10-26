//! Taking over the terminal and giving it back.

use base64::Engine;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::DefaultTerminal;

/// Raw mode, alternate screen, bracketed paste (a pasted path arrives in one piece, so it can
/// become an attachment) and the mouse (selecting within a pane, the wheel). Shift-drag still
/// selects the terminal's way in most terminals.
pub fn init() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableBracketedPaste, EnableMouseCapture);
    terminal
}

pub fn restore() {
    let _ = crossterm::execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
    ratatui::restore();
}

/// Puts `text` on the clipboard: through the terminal (OSC 52, which reaches the clipboard of
/// the machine you sit at, over ssh too) and `clipboard`, the system's here.
pub fn copy(text: &str, clipboard: &mut Option<arboard::Clipboard>) {
    use std::io::Write;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{encoded}\x07");
    let _ = out.flush();
    // Kept: on Linux the text is served from it, and goes when it is dropped.
    if clipboard.is_none() {
        *clipboard = arboard::Clipboard::new().ok();
    }
    // No display (a bare tty, ssh): OSC 52 did it, or nothing can.
    if let Some(clipboard) = clipboard {
        let _ = clipboard.set_text(text);
    }
}
