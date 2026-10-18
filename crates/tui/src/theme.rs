//! Colour tokens from `docs/mockups/theme.css`. The TUI has no rounded corners.

use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Rgb(0x0c, 0x0d, 0x0f);
pub const SURFACE: Color = Color::Rgb(0x12, 0x13, 0x17);
pub const RAISED: Color = Color::Rgb(0x19, 0x1b, 0x20);
/// Background of your own messages: `bg` tinted toward the accent.
pub const YOU: Color = Color::Rgb(0x15, 0x1a, 0x22);

pub const BORDER: Color = Color::Rgb(0x24, 0x27, 0x2d);
pub const BORDER_STRONG: Color = Color::Rgb(0x36, 0x3a, 0x42);

pub const TEXT: Color = Color::Rgb(0xd5, 0xd8, 0xde);
pub const MUTED: Color = Color::Rgb(0x86, 0x8c, 0x96);
pub const FAINT: Color = Color::Rgb(0x50, 0x55, 0x5e);

pub const ACCENT: Color = Color::Rgb(0x8f, 0xb3, 0xe8);
pub const OK: Color = Color::Rgb(0x8c, 0xc2, 0x9a);
pub const WARN: Color = Color::Rgb(0xd6, 0xb0, 0x6a);
pub const ERR: Color = Color::Rgb(0xe0, 0x7c, 0x7c);
/// The agent: tool rows, thinking, agent labels.
pub const INFO: Color = Color::Rgb(0xb3, 0x9c, 0xe0);

pub fn text() -> Style {
    Style::new().fg(TEXT)
}

pub fn muted() -> Style {
    Style::new().fg(MUTED)
}

pub fn faint() -> Style {
    Style::new().fg(FAINT)
}

pub fn link() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::UNDERLINED)
}

pub fn bold() -> Style {
    Style::new().fg(TEXT).add_modifier(Modifier::BOLD)
}
