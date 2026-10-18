use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, KeyOutcome};
use crate::store::types::{Mode, Pane, PickerKind};

type Chord = (KeyCode, KeyModifiers);

/// Normal-mode bindings, including multi-key sequences (`Space p`, `ctrl-w o`).
pub struct Keymap {
    bindings: Vec<(Vec<Chord>, Action)>,
    pending: Vec<Chord>,
}

impl Keymap {
    pub fn defaults() -> Self {
        let ctrl = |c| (KeyCode::Char(c), KeyModifiers::CONTROL);
        let key = |c| (KeyCode::Char(c), KeyModifiers::NONE);
        let space = (KeyCode::Char(' '), KeyModifiers::NONE);
        let bindings = vec![
            (vec![key('i')], Action::SetMode(Mode::Insert)),
            (vec![key(':')], Action::SetMode(Mode::Command)),
            (vec![key('f')], Action::SetMode(Mode::Hint)),
            (vec![(KeyCode::Tab, KeyModifiers::NONE)], Action::FocusNext),
            (vec![ctrl('w'), key('h')], Action::FocusPane(Pane::Chat)),
            (vec![ctrl('w'), key('l')], Action::FocusPane(Pane::Page)),
            (vec![ctrl('w'), key('w')], Action::FocusNext),
            (vec![ctrl('w'), key('o')], Action::ToggleMaximize),
            (vec![key('H')], Action::PageBack),
            (vec![key('L')], Action::PageForward),
            (vec![key('F')], Action::ToggleFollow),
            (vec![key('r')], Action::ReloadPage),
            (vec![key('e')], Action::EditPage),
            (vec![key('a')], Action::OpenApp(None)),
            (vec![key('t')], Action::ToggleThinking),
            (vec![key('T')], Action::ToggleToolDetails),
            (vec![ctrl('c')], Action::CancelTurn),
            (vec![ctrl('o')], Action::JumpBack),
            (vec![space, key('p')], Action::OpenPicker(PickerKind::Pages)),
            (vec![space, key('s')], Action::OpenPicker(PickerKind::Sites)),
            (
                vec![space, key('w')],
                Action::OpenPicker(PickerKind::Sessions),
            ),
            (vec![space, key('/')], Action::OpenPicker(PickerKind::Pages)),
        ];
        Self {
            bindings,
            pending: Vec::new(),
        }
    }

    pub fn feed(&mut self, key: KeyEvent) -> KeyOutcome {
        self.pending.push(normalize(key));
        if let Some((_, action)) = self.bindings.iter().find(|(seq, _)| *seq == self.pending) {
            self.pending.clear();
            return KeyOutcome::Action(action.clone());
        }
        if self
            .bindings
            .iter()
            .any(|(seq, _)| seq.starts_with(&self.pending))
        {
            return KeyOutcome::Pending;
        }
        let was_sequence = self.pending.len() > 1;
        self.pending.clear();
        // An unknown second key cancels the sequence instead of reaching a component.
        if was_sequence {
            KeyOutcome::Pending
        } else {
            KeyOutcome::Unbound
        }
    }
}

/// Shifted letters arrive as `Char('H') + SHIFT`; bindings are written without SHIFT.
fn normalize(key: KeyEvent) -> Chord {
    match key.code {
        KeyCode::Char(c) => (KeyCode::Char(c), key.modifiers - KeyModifiers::SHIFT),
        code => (code, key.modifiers),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn sequences() {
        let mut km = Keymap::defaults();
        assert_eq!(km.feed(k(' ')), KeyOutcome::Pending);
        assert_eq!(
            km.feed(k('p')),
            KeyOutcome::Action(Action::OpenPicker(PickerKind::Pages))
        );
        assert_eq!(km.feed(k('j')), KeyOutcome::Unbound);
        assert_eq!(km.feed(k(' ')), KeyOutcome::Pending);
        assert_eq!(
            km.feed(k('x')),
            KeyOutcome::Pending,
            "bad sequence is swallowed"
        );
        let shift_h = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT);
        assert_eq!(km.feed(shift_h), KeyOutcome::Action(Action::PageBack));
    }
}
