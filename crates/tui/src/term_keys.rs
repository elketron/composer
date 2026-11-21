//! A crossterm key as the terminal model's key (`composer-term`'s key map, by name).

use composer_term::key_map::{Held, terminal_key};
use composer_term::{KeyCode as TermKey, KeyModifiers as TermMods};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The terminal model's key for `key`; `None` for keys it has none for (a bare modifier).
pub fn term_key(key: KeyEvent) -> Option<(TermKey, TermMods)> {
    let mut held = Held {
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
        control: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        platform: key.modifiers.contains(KeyModifiers::SUPER),
    };
    let typed;
    let name: String = match key.code {
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "escape".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => {
            held.shift = true;
            "tab".into()
        }
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Char(' ') => {
            typed = Some(" ".to_string());
            return terminal_key("space", typed.as_deref(), held);
        }
        KeyCode::Char(c) => {
            typed = Some(c.to_string());
            return terminal_key(&c.to_lowercase().to_string(), typed.as_deref(), held);
        }
        _ => return None,
    };
    terminal_key(&name, None, held)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<(TermKey, TermMods)> {
        term_key(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn keys() {
        assert_eq!(
            key(KeyCode::Char('A'), KeyModifiers::SHIFT),
            Some((TermKey::Char('A'), TermMods::NONE))
        );
        assert_eq!(
            key(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some((TermKey::Char('c'), TermMods::CTRL))
        );
        assert_eq!(
            key(KeyCode::Enter, KeyModifiers::NONE),
            Some((TermKey::Enter, TermMods::NONE))
        );
        assert_eq!(
            key(KeyCode::BackTab, KeyModifiers::SHIFT),
            Some((TermKey::Tab, TermMods::SHIFT))
        );
        assert_eq!(
            key(KeyCode::F(5), KeyModifiers::NONE),
            Some((TermKey::Function(5), TermMods::NONE))
        );
        assert_eq!(key(KeyCode::CapsLock, KeyModifiers::NONE), None);
    }
}
