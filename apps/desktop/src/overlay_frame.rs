//! Where a floating pane (the palette, a picker) sits: dead centre of the window's area between
//! the title bar and the status bar, on the session page and on Home alike.

/// The panel's width and the most height it may take, in an area of the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayFrame {
    width: f32,
    max_height: f32,
}

/// The panel is never wider than this (the mockups' chat column, 640, less 2 × 54).
pub const MAX_WIDTH: f32 = 532.0;
/// Room kept above and below the panel, so it never touches the bars.
pub const MARGIN_Y: f32 = 24.0;

impl OverlayFrame {
    /// The frame for an area `width` × `height` (the window less its two bars).
    pub fn within(width: f32, height: f32) -> Self {
        let inset = Self::inset(width);
        Self {
            width: (width - 2.0 * inset).clamp(0.0, MAX_WIDTH),
            max_height: (height - 2.0 * MARGIN_Y).max(0.0),
        }
    }

    /// The least room left and right of the panel: `base.css`'s `.overlay` steps (54px, 24px at
    /// 1200px and below, 16px at 900px and below).
    pub fn inset(width: f32) -> f32 {
        if width > 1200.0 {
            54.0
        } else if width > 900.0 {
            24.0
        } else {
            16.0
        }
    }

    pub fn width(self) -> f32 {
        self.width
    }

    pub fn max_height(self) -> f32 {
        self.max_height
    }

    /// Its left edge in an area `width` wide: centred, the same room on both sides.
    pub fn left(self, width: f32) -> f32 {
        (width - self.width) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centred_with_equal_room_on_both_sides() {
        let frame = OverlayFrame::within(1600.0, 930.0);
        assert_eq!(
            frame.width(),
            MAX_WIDTH,
            "a wide window: the panel's own width"
        );
        assert_eq!(frame.left(1600.0), 534.0);
        assert_eq!(
            frame.left(1600.0),
            1600.0 - frame.left(1600.0) - frame.width(),
            "as much room right as left"
        );
        assert_eq!(frame.max_height(), 930.0 - 2.0 * MARGIN_Y);
    }

    #[test]
    fn narrow_windows_step_the_insets_down() {
        assert_eq!(OverlayFrame::inset(1201.0), 54.0);
        assert_eq!(OverlayFrame::inset(1200.0), 24.0);
        assert_eq!(OverlayFrame::inset(901.0), 24.0);
        assert_eq!(OverlayFrame::inset(900.0), 16.0);
        let frame = OverlayFrame::within(560.0, 400.0);
        assert_eq!(frame.width(), 560.0 - 32.0, "16px each side");
        assert_eq!(frame.left(560.0), 16.0);
    }

    #[test]
    fn never_leaves_the_area_or_covers_the_bars() {
        // The narrowest window the panes allow: two 320px minimums.
        let width = 2.0 * crate::chat_width::MIN;
        for height in [0.0, 30.0, 200.0, 900.0] {
            let frame = OverlayFrame::within(width, height);
            assert!(frame.left(width) >= 0.0);
            assert!(frame.left(width) + frame.width() <= width);
            assert!(frame.max_height() <= height, "inside the area at {height}");
            assert!(frame.max_height() >= 0.0);
        }
        assert_eq!(
            OverlayFrame::within(10.0, 10.0).width(),
            0.0,
            "never negative"
        );
    }
}
