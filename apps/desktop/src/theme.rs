//! The desktop's colours and corners: the Noema palette (docs/mockups/next/tokens.css) and the
//! phone's radii (apps/composer_app/lib/theme/tokens.dart). Bars, tabs and the stage stay square.

/// Default native desktop text, with a smaller size for metadata and key hints.
pub const TEXT_SIZE: f32 = 14.;
pub const TEXT_SMALL: f32 = 12.;

/// The window, the chat.
pub const BG: u32 = 0x0e0e0f;
/// The stage behind its tabs, a scrim's colour.
pub const BG_DEEP: u32 = 0x0a0a0b;
/// The title and status bars, floating panels.
pub const PANEL: u32 = 0x111112;
pub const PANEL_2: u32 = 0x131315;
/// A selected row.
pub const SURFACE_2: u32 = 0x1a1a1c;
/// The composer.
pub const SURFACE: u32 = 0x171718;
pub const BORDER: u32 = 0x222224;
pub const BORDER_2: u32 = 0x2a2a2c;
pub const FG: u32 = 0xc2c0b6;
pub const FG_BRIGHT: u32 = 0xd8d6cc;
pub const FG_DIM: u32 = 0x8a8880;
pub const FG_FAINT: u32 = 0x6a6865;
/// Focus, the active tab's edge, the selected row's bar.
pub const ACCENT: u32 = 0x4e46a0;
pub const ACCENT_HOVER: u32 = 0x5a52b0;
/// Text in the accent: names, "shown", unseen tabs.
pub const ACCENT_SOFT: u32 = 0x8a82cc;
/// An approval's card, and its edge; the agent's rule in the transcript.
pub const ACCENT_DEEP: u32 = 0x12111a;
pub const ACCENT_BORDER: u32 = 0x2a2740;
pub const OK: u32 = 0x4caf82;
/// Your rule in the transcript.
pub const OK_TEXT: u32 = 0x3d8a65;
pub const WARN: u32 = 0xd4a843;
/// A waiting verdict's edge.
pub const WARN_DEEP: u32 = 0x8a6e30;
pub const ERR: u32 = 0xe05a5a;
pub const ERR_TEXT: u32 = 0xc05050;

/// Cards and floating panels.
pub const R_CARD: f32 = 5.;
/// Buttons and fields.
pub const R_CONTROL: f32 = 4.;
/// Keys and chips.
pub const R_SM: f32 = 3.;
/// Status dots.
pub const R_DOT: f32 = 2.;

/// How much of the chat a scrim lets through.
pub const SCRIM_ALPHA: f32 = 0.72;
