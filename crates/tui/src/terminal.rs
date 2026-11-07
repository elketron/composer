//! Taking over the terminal and giving it back.

use base64::Engine;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::DefaultTerminal;

/// Raw mode, alternate screen, bracketed paste (a pasted or dropped path arrives in one
/// piece, so it can become an attachment) and the mouse (selecting within a pane, the wheel). Shift-drag still
/// selects the terminal's way in most terminals. Where the terminal speaks the kitty keyboard
/// protocol, keys come unambiguous: Escape followed quickly by a key isn't read as Alt+key.
pub fn init() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableBracketedPaste, EnableMouseCapture);
    if crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false) {
        let _ = crossterm::execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    terminal
}

pub fn restore() {
    // Popping when nothing was pushed is harmless.
    let _ = crossterm::execute!(
        std::io::stdout(),
        PopKeyboardEnhancementFlags,
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
