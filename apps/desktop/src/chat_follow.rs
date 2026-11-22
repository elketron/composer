//! Whether the chat keeps its newest content in view, decided without a window.
//!
//! The chat used to ask, at each new event, whether its view was at the end, measured from the
//! last layout. Anything that grew the transcript or shrank its view between two events (the
//! composer growing a line, an approval's bar, a block re-wrapping) left the view more than a
//! line above the end, and from then on new messages arrived out of sight. Now the chat is
//! pinned to the end until the reader scrolls away, and every draw while pinned shows the end.

/// How close to the end (in pixels) a scroll must stop to pin the view again.
pub const AT_END: f32 = 4.;

#[derive(Debug)]
pub struct Follow {
    /// The view shows the end, and keeps showing it as the chat grows.
    pinned: bool,
    /// The reader scrolled: on the next draw, pinned again only if the scroll ended at the end.
    scrolled: bool,
}

impl Default for Follow {
    fn default() -> Self {
        Self {
            pinned: true,
            scrolled: false,
        }
    }
}

impl Follow {
    /// To the end, and stay there (a message sent, a session opened, `G`, an approval).
    pub fn stick_to_end(&mut self) {
        self.pinned = true;
        self.scrolled = false;
    }

    /// Away from the end on purpose (`g`, earlier messages loaded).
    pub fn away(&mut self) {
        self.pinned = false;
        self.scrolled = false;
    }

    /// The reader turned the wheel or dragged; where it ends is known at the next draw.
    pub fn scrolled(&mut self) {
        self.scrolled = true;
    }

    /// A new item in the transcript (a message, a tool call, a note). While the client follows
    /// the agent, it brings the view back to the end; otherwise only a pinned view follows.
    pub fn new_item(&mut self, following_agent: bool) {
        if following_agent {
            self.stick_to_end();
        }
    }

    /// A draw: `distance` is how far the view is from the end. `true` when the view must go
    /// to the end.
    pub fn frame(&mut self, distance: f32) -> bool {
        if std::mem::take(&mut self.scrolled) {
            self.pinned = distance <= AT_END;
        }
        self.pinned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_stays_at_the_end_however_far_it_grew() {
        let mut follow = Follow::default();
        // The composer grew, an approval's bar came: the end is far below; still followed.
        assert!(follow.frame(0.));
        assert!(follow.frame(180.));
        assert!(follow.frame(2_000.));
    }

    #[test]
    fn scrolling_up_unpins_and_back_down_pins() {
        let mut follow = Follow::default();
        follow.scrolled();
        assert!(!follow.frame(120.), "scrolled away to read");
        assert!(!follow.frame(400.), "new text doesn't pull the reader down");
        follow.scrolled();
        assert!(follow.frame(0.), "scrolled back to the end");
        assert!(follow.frame(300.));
    }

    #[test]
    fn a_scroll_that_stays_at_the_end_keeps_it_pinned() {
        let mut follow = Follow::default();
        follow.scrolled();
        assert!(follow.frame(AT_END));
    }

    #[test]
    fn following_the_agent_brings_new_items_into_view() {
        let mut follow = Follow::default();
        follow.away();
        follow.new_item(false);
        assert!(!follow.frame(500.), "not following: the reader stays");
        follow.new_item(true);
        assert!(follow.frame(500.), "following: the new message shows");
    }

    #[test]
    fn sending_or_jumping_pins_going_to_the_top_unpins() {
        let mut follow = Follow::default();
        follow.away();
        assert!(!follow.frame(50.));
        follow.stick_to_end();
        assert!(follow.frame(50.));
        follow.scrolled();
        follow.away();
        assert!(
            !follow.frame(0.),
            "an explicit move wins over a pending scroll"
        );
    }
}
