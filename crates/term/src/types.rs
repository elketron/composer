//! What a UI paints: a cell's text and style.

/// A colour as the program set it: the UI's default, one of the 256 palette colours (so a UI
/// can keep its own theme for them), or an exact one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Paint {
    #[default]
    Default,
    Index(u8),
    Rgb(u8, u8, u8),
}

/// One cell of the screen: its text (empty for blank), where it starts, how many columns it
/// takes, and its style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CellView {
    pub col: usize,
    pub text: String,
    pub width: usize,
    pub fg: Paint,
    pub bg: Paint,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Foreground and background swapped.
    pub reverse: bool,
    pub invisible: bool,
}
