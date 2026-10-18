//! Shared drawing helpers.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

use crate::theme;

/// A square-cornered pane; the title sits in the top border.
pub fn pane_block(title: Line<'static>, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(if focused {
            theme::BORDER_STRONG
        } else {
            theme::BORDER
        }))
        .title(title)
        .style(Style::new().bg(theme::BG))
}

/// Dims everything under a floating window.
pub fn dim(buffer: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            cell.set_fg(theme::FAINT);
            if cell.bg != theme::BG {
                cell.set_bg(theme::BG);
            }
        }
    }
}

/// A centered rect of at most `width` × `height`, placed in the upper part of `area`.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height.saturating_sub(height) / 5).max(1),
        width,
        height,
    }
}

/// `~/Projects/composer` for paths under `$HOME`.
pub fn short_path(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if path.starts_with(&home) => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    }
}

/// Pads a line to `width` with `bg`, so message blocks read as full-width bands.
pub fn band(mut line: Line<'static>, width: u16, bg: ratatui::style::Color) -> Line<'static> {
    let used = line.width() as u16;
    line.spans.insert(0, Span::raw(" "));
    if used + 1 < width {
        line.spans
            .push(Span::raw(" ".repeat((width - used - 1) as usize)));
    }
    line.style(Style::new().bg(bg))
}
