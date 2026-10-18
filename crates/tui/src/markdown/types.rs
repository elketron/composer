use ratatui::style::Style;
use ratatui::text::Line;

/// Markdown rendered for a given width.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub lines: Vec<Line<'static>>,
    pub links: Vec<RenderedLink>,
}

/// Where a link landed on screen. A link wrapped across lines appears once per line.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedLink {
    /// Index of the link in the document (shared by its wrapped parts).
    pub id: usize,
    pub line: usize,
    pub start: u16,
    pub end: u16,
    /// Absolute: `site://...` resolved against the page, or an external URL.
    pub url: String,
}

/// A run of text with one style, possibly part of a link (renderer internal).
#[derive(Debug, Clone)]
pub(crate) struct Seg {
    pub text: String,
    pub style: Style,
    pub link: Option<usize>,
}
