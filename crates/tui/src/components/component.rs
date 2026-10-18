use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::action::Action;
use crate::store::AppState;

/// A piece of UI. Keeps only local view state (scroll, cursor, input buffer);
/// everything shared lives in the stores and changes through actions.
pub trait Component {
    /// Keys the keymap didn't bind, or all keys in the component's own mode.
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action>;

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState);
}
