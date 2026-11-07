/// The chat column's width: dragged by its edge, never closed, and leaving the stage room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatWidth(f32);

/// Neither side gets narrower than this.
pub const MIN: f32 = 320.0;

impl ChatWidth {
    pub const DEFAULT: Self = Self(640.0);

    pub fn get(self) -> f32 {
        self.0
    }

    /// Its width when dragged to `x` in a window `total` wide.
    pub fn dragged_to(x: f32, total: f32) -> Self {
        Self(x.min(total - MIN).max(MIN))
    }

    /// Kept inside a window that is now `total` wide.
    pub fn within(self, total: f32) -> Self {
        Self::dragged_to(self.0, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_between_the_minimums() {
        assert_eq!(ChatWidth::dragged_to(500.0, 1500.0).get(), 500.0);
        assert_eq!(
            ChatWidth::dragged_to(10.0, 1500.0).get(),
            MIN,
            "never closed"
        );
        assert_eq!(
            ChatWidth::dragged_to(1400.0, 1500.0).get(),
            1500.0 - MIN,
            "the stage keeps room"
        );
        assert_eq!(ChatWidth::DEFAULT.within(800.0).get(), 800.0 - MIN);
        assert_eq!(
            ChatWidth::dragged_to(500.0, 500.0).get(),
            MIN,
            "a tiny window: the chat wins"
        );
    }
}
