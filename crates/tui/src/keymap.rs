use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::{Action, KeyOutcome};
use crate::store::types::{Mode, Pane, PickerKind};

type Chord = (KeyCode, KeyModifiers);

/// Normal-mode bindings, including multi-key sequences (`Space p`, `ctrl-w o`).
/// Each has a description: help is generated from this list.
pub struct Keymap {
    bindings: Vec<(Vec<Chord>, Action, &'static str)>,
    pending: Vec<Chord>,
    /// The keys of `pending`, to hand back when the sequence turns out unbound.
    pending_keys: Vec<KeyEvent>,
}

impl Keymap {
    pub fn defaults() -> Self {
        let ctrl = |c| (KeyCode::Char(c), KeyModifiers::CONTROL);
        let key = |c| (KeyCode::Char(c), KeyModifiers::NONE);
        let space = (KeyCode::Char(' '), KeyModifiers::NONE);
        let tab = (KeyCode::Tab, KeyModifiers::NONE);
        let bindings = vec![
            (vec![key('?')], Action::ToggleHelp, "this help"),
            (
                vec![key('i')],
                Action::SetMode(Mode::Insert),
                "write a message (Insert mode)",
            ),
            (
                vec![key(':')],
                Action::SetMode(Mode::Command),
                "command line",
            ),
            (
                vec![key('f')],
                Action::SetMode(Mode::Hint),
                "label the page's links; type a label to follow it",
            ),
            (
                vec![space, key('p')],
                Action::OpenPicker(PickerKind::Pages),
                "find a page",
            ),
            (
                vec![space, key('/')],
                Action::OpenPicker(PickerKind::Pages),
                "find a page (search)",
            ),
            (
                vec![space, key('s')],
                Action::OpenPicker(PickerKind::Sites),
                "sites",
            ),
            (
                vec![space, key('w')],
                Action::OpenPicker(PickerKind::Sessions),
                "sessions and workers",
            ),
            (
                vec![space, key('o')],
                Action::OpenPicker(PickerKind::Orchestration),
                "this orchestration: orchestrator and workers, with what each is doing",
            ),
            (
                vec![key(']'), key('w')],
                Action::CycleOrchestration(1),
                "next session of this orchestration",
            ),
            (
                vec![key('['), key('w')],
                Action::CycleOrchestration(-1),
                "previous session of this orchestration",
            ),
            (vec![tab], Action::FocusNext, "switch pane"),
            (
                vec![ctrl('w'), key('h')],
                Action::FocusPane(Pane::Chat),
                "focus the chat",
            ),
            (
                vec![ctrl('w'), key('l')],
                Action::FocusPane(Pane::Page),
                "focus the page",
            ),
            (vec![ctrl('w'), key('w')], Action::FocusNext, "switch pane"),
            (
                vec![ctrl('w'), key('o')],
                Action::ToggleMaximize,
                "maximize / restore the pane",
            ),
            (vec![key('H')], Action::PageBack, "page history: back"),
            (vec![key('L')], Action::PageForward, "page history: forward"),
            (
                vec![key('F')],
                Action::ToggleFollow,
                "follow the agent's pages on / off",
            ),
            (
                vec![key('r')],
                Action::ReloadPage,
                "reload the page (or the changes view)",
            ),
            (vec![key('e')], Action::EditPage, "edit the page in $EDITOR"),
            (
                vec![key('a')],
                Action::OpenApp(None),
                "open the page's site app in the browser",
            ),
            (
                vec![key('g'), key('d')],
                Action::ToggleChanges,
                "changes of the active project (git)",
            ),
            (
                vec![key('t')],
                Action::ToggleThinking,
                "show / hide thinking",
            ),
            (
                vec![key('T')],
                Action::ToggleToolDetails,
                "show / hide tool results and summaries",
            ),
            (
                vec![ctrl('c')],
                Action::CancelTurn,
                "cancel the agent's turn",
            ),
            (
                vec![ctrl('o')],
                Action::JumpBack,
                "back to the previous session (worker → orchestrator)",
            ),
        ];
        Self {
            bindings,
            pending: Vec::new(),
            pending_keys: Vec::new(),
        }
    }

    /// Every binding as (keys, description), e.g. (`space p`, `find a page`).
    pub fn help(&self) -> Vec<(String, &'static str)> {
        self.bindings
            .iter()
            .map(|(chords, _, description)| {
                let names: Vec<String> = chords.iter().map(chord_name).collect();
                // Plain letters run together like vim writes them (`gd`); others get spaces (`space p`).
                let plain = names.iter().all(|n| n.chars().count() == 1);
                (names.join(if plain { "" } else { " " }), *description)
            })
            .collect()
    }

    pub fn feed(&mut self, key: KeyEvent) -> KeyOutcome {
        self.pending.push(normalize(key));
        self.pending_keys.push(key);
        if let Some((_, action, _)) = self
            .bindings
            .iter()
            .find(|(seq, _, _)| *seq == self.pending)
        {
            self.pending.clear();
            self.pending_keys.clear();
            return KeyOutcome::Action(action.clone());
        }
        if self
            .bindings
            .iter()
            .any(|(seq, _, _)| seq.starts_with(&self.pending))
        {
            return KeyOutcome::Pending;
        }
        self.pending.clear();
        let keys = std::mem::take(&mut self.pending_keys);
        // Not a binding after all: the component gets the keys (so `gg` still scrolls to the top).
        if keys.len() > 1 {
            KeyOutcome::Replay(keys)
        } else {
            KeyOutcome::Unbound
        }
    }
}

/// `space`, `ctrl-w`, `H`, `tab`: how keys are written in help.
fn chord_name(chord: &Chord) -> String {
    let key = match chord.0 {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        other => format!("{other:?}").to_lowercase(),
    };
    if chord.1.contains(KeyModifiers::CONTROL) {
        format!("ctrl-{key}")
    } else {
        key
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
            KeyOutcome::Replay(vec![k(' '), k('x')]),
            "unbound sequences go to the component"
        );
        assert_eq!(km.feed(k('g')), KeyOutcome::Pending);
        assert_eq!(
            km.feed(k('g')),
            KeyOutcome::Replay(vec![k('g'), k('g')]),
            "gg still scrolls"
        );
        assert_eq!(km.feed(k('g')), KeyOutcome::Pending);
        assert_eq!(km.feed(k('d')), KeyOutcome::Action(Action::ToggleChanges));
        let shift_h = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT);
        assert_eq!(km.feed(shift_h), KeyOutcome::Action(Action::PageBack));
    }

    #[test]
    fn help_lists_every_binding_readably() {
        let help = Keymap::defaults().help();
        assert!(
            help.iter()
                .all(|(keys, description)| !keys.is_empty() && !description.is_empty())
        );
        let keys: Vec<&str> = help.iter().map(|(k, _)| k.as_str()).collect();
        for expected in ["?", "space p", "ctrl-w o", "gd", "H", "ctrl-c", "tab"] {
            assert!(keys.contains(&expected), "{expected} missing from {keys:?}");
        }
    }
}
