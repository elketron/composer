//! Where the keyboard belongs, decided without a window: [`crate::shell::Shell`] says what is on
//! screen and who has the keyboard; this says where it must go instead (if anywhere).
//!
//! Without it the keyboard could stay with something no longer drawn (a picker that closed, a
//! terminal whose tab closed or is hidden): no pane got keys, and the window's own shortcuts
//! (the palette, the sessions) went nowhere either.

use gpui::{Pixels, Point};

/// Who has the keyboard now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    /// The palette or a picker that is open.
    Overlay,
    Home,
    Chat,
    /// The stage's shown tab (its terminal or browser pane).
    ShownStage,
    /// A stage pane whose tab is not the shown one.
    HiddenStage,
    /// Nobody, or something not drawn any more.
    Other,
}

/// Where the keyboard must go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Overlay,
    Home,
    Chat,
    ShownStage,
}

/// What is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scene {
    pub overlay: bool,
    pub home: bool,
    /// The stage's shown tab has a pane that can take keys.
    pub stage: bool,
}

/// Where the keyboard must go, or `None` when it is fine where it is.
pub fn target(scene: Scene, holder: Holder) -> Option<Target> {
    let want = if scene.overlay {
        Target::Overlay
    } else if scene.home {
        Target::Home
    } else {
        match holder {
            Holder::Chat => return None,
            Holder::ShownStage if scene.stage => return None,
            // The shown tab changed (ctrl+tab, the palette, the agent) while a pane now hidden
            // had the keyboard: the shown pane takes it.
            Holder::HiddenStage if scene.stage => Target::ShownStage,
            _ => Target::Chat,
        }
    };
    let already = matches!(
        (want, holder),
        (Target::Overlay, Holder::Overlay) | (Target::Home, Holder::Home)
    );
    (!already).then_some(want)
}

/// "The panel follows the pointer": moving the pointer over a panel gives it the keyboard. Only
/// a real move does: the pointer arriving where it already was when the window opens or comes
/// back to the front (gpui reports that as a move), or a nudge of a pixel, must not take the
/// keyboard from the chat; once the pointer has travelled [`TRAVEL`] pixels, every move counts.
#[derive(Debug, Default)]
pub struct HoverFocus {
    /// Where the pointer was first seen (since the window opened or came to the front).
    arrived: Option<Point<Pixels>>,
    travelled: bool,
}

/// How far (in pixels) the pointer must move before the panels follow it.
pub const TRAVEL: f32 = 8.;

impl HoverFocus {
    /// A pointer event at `at`: `true` when the pointer is moving, so the panel under it takes
    /// the keys.
    pub fn moves_to(&self, at: Point<Pixels>) -> bool {
        self.travelled || self.arrived.is_some_and(|from| far(from, at))
    }

    /// Where the pointer is now (after the panels have seen the event).
    pub fn seen_at(&mut self, at: Point<Pixels>) {
        match self.arrived {
            None => self.arrived = Some(at),
            Some(from) => self.travelled |= far(from, at),
        }
    }

    /// The window opened or came back to the front: the pointer has to move again.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

fn far(from: Point<Pixels>, to: Point<Pixels>) -> bool {
    let (dx, dy) = (f32::from(to.x - from.x), f32::from(to.y - from.y));
    dx.hypot(dy) >= TRAVEL
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    const STAGE: Scene = Scene {
        overlay: false,
        home: false,
        stage: true,
    };

    #[test]
    fn an_open_picker_has_the_keyboard() {
        let scene = Scene {
            overlay: true,
            ..STAGE
        };
        for holder in [
            Holder::Chat,
            Holder::ShownStage,
            Holder::HiddenStage,
            Holder::Other,
        ] {
            assert_eq!(target(scene, holder), Some(Target::Overlay), "{holder:?}");
        }
        assert_eq!(target(scene, Holder::Overlay), None);
    }

    #[test]
    fn the_home_has_the_keyboard_while_shown() {
        let scene = Scene {
            home: true,
            ..STAGE
        };
        assert_eq!(target(scene, Holder::Other), Some(Target::Home));
        assert_eq!(target(scene, Holder::Home), None);
    }

    #[test]
    fn the_chat_and_the_shown_tab_keep_it() {
        assert_eq!(target(STAGE, Holder::Chat), None);
        assert_eq!(target(STAGE, Holder::ShownStage), None);
    }

    #[test]
    fn nobody_or_something_gone_gives_it_to_the_chat() {
        // At start, after a picker closed onto a pane that is gone, after the home closed.
        assert_eq!(target(STAGE, Holder::Other), Some(Target::Chat));
        assert_eq!(target(STAGE, Holder::Overlay), Some(Target::Chat));
        assert_eq!(target(STAGE, Holder::Home), Some(Target::Chat));
        let empty = Scene::default();
        assert_eq!(target(empty, Holder::Other), Some(Target::Chat));
    }

    #[test]
    fn a_hidden_pane_hands_it_to_the_shown_one() {
        assert_eq!(target(STAGE, Holder::HiddenStage), Some(Target::ShownStage));
        // Nothing on the stage takes keys (a tab still opening): the chat does.
        let opening = Scene::default();
        assert_eq!(target(opening, Holder::HiddenStage), Some(Target::Chat));
        assert_eq!(target(opening, Holder::ShownStage), Some(Target::Chat));
    }

    #[test]
    fn the_pointer_arriving_is_no_move() {
        let mut hover = HoverFocus::default();
        let event = |hover: &mut HoverFocus, x: f32| {
            let at = point(px(x), px(500.));
            let moved = hover.moves_to(at);
            hover.seen_at(at);
            moved
        };
        assert!(!event(&mut hover, 900.), "it arrives");
        assert!(!event(&mut hover, 900.), "still there");
        assert!(!event(&mut hover, 901.5), "a nudge");
        assert!(event(&mut hover, 920.), "it moved");
        assert!(event(&mut hover, 920.5), "and keeps counting");
        hover.reset();
        assert!(!event(&mut hover, 10.), "back to the front");
        assert!(!event(&mut hover, 12.));
        assert!(event(&mut hover, 30.));
    }
}
