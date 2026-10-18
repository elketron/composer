//! Terminal buffer → HTML (with colours, for screenshots) and plain text.

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use crate::theme;

/// A standalone HTML page showing the buffer cell by cell.
pub fn buffer_to_html(buffer: &Buffer, title: &str) -> String {
    let mut rows = String::new();
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        let mut row = String::new();
        let mut run = String::new();
        let mut run_style: Option<String> = None;
        for x in area.left()..area.right() {
            let cell = &buffer[(x, y)];
            let style = cell_style(cell.fg, cell.bg, cell.modifier);
            if run_style.as_ref() != Some(&style) {
                if let Some(s) = run_style.take() {
                    row.push_str(&format!("<span style=\"{s}\">{}</span>", escape(&run)));
                }
                run.clear();
                run_style = Some(style);
            }
            run.push_str(cell.symbol());
        }
        if let Some(s) = run_style {
            row.push_str(&format!("<span style=\"{s}\">{}</span>", escape(&run)));
        }
        rows.push_str(&format!("<div>{row}</div>"));
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=JetBrains+Mono:ital,wght@0,400;0,700;1,400&display=swap\">\
         <style>body{{margin:0;padding:16px;background:{bg}}}\
         .t{{font:13px/1.35 'JetBrains Mono',monospace;color:{fg};display:inline-block}}\
         .t div{{height:1.35em;white-space:pre}}</style></head><body><div class=\"t\">{rows}</div></body></html>\n",
        bg = hex(theme::BG).unwrap_or_default(),
        fg = hex(theme::TEXT).unwrap_or_default(),
    )
}

/// The buffer as plain text, trailing spaces trimmed.
pub fn buffer_to_text(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        let line: String = (area.left()..area.right())
            .map(|x| &buffer[(x, y)])
            .map(|c| c.symbol())
            .collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn cell_style(fg: Color, bg: Color, modifier: Modifier) -> String {
    let mut style = String::new();
    let (mut fg, mut bg) = (hex(fg), hex(bg));
    if modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if let Some(fg) = fg {
        style.push_str(&format!("color:{fg};"));
    }
    if let Some(bg) = bg {
        style.push_str(&format!("background:{bg};"));
    }
    if modifier.contains(Modifier::BOLD) {
        style.push_str("font-weight:700;");
    }
    if modifier.contains(Modifier::ITALIC) {
        style.push_str("font-style:italic;");
    }
    if modifier.contains(Modifier::UNDERLINED) {
        style.push_str("text-decoration:underline;text-underline-offset:3px;");
    }
    if modifier.contains(Modifier::CROSSED_OUT) {
        style.push_str("text-decoration:line-through;");
    }
    style
}

fn hex(color: Color) -> Option<String> {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Reset => return None,
        Color::Black => (0, 0, 0),
        Color::White => (255, 255, 255),
        Color::Red | Color::LightRed => (224, 124, 124),
        Color::Green | Color::LightGreen => (140, 194, 154),
        Color::Yellow | Color::LightYellow => (214, 176, 106),
        Color::Blue | Color::LightBlue => (143, 179, 232),
        Color::Magenta | Color::LightMagenta => (179, 156, 224),
        Color::Cyan | Color::LightCyan => (120, 200, 210),
        Color::Gray => (134, 140, 150),
        Color::DarkGray => (80, 85, 94),
        Color::Indexed(_) => return None,
    };
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
