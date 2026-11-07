//! A gpui keystroke as the terminal model's key: special keys by name, text by the typed character
//! (layout-aware), with ctrl/alt by the key itself.

use tattoy_wezterm_term::{KeyCode, KeyModifiers};

/// The keystroke's parts this needs (so it can be tested without gpui).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub platform: bool,
}

/// `key` is gpui's key name ("a", "enter", "f5"), `typed` the character it produced, if any.
pub fn terminal_key(key: &str, typed: Option<&str>, held: Held) -> Option<(KeyCode, KeyModifiers)> {
    let mut mods = KeyModifiers::NONE;
    if held.shift {
        mods |= KeyModifiers::SHIFT;
    }
    if held.control {
        mods |= KeyModifiers::CTRL;
    }
    if held.alt {
        mods |= KeyModifiers::ALT;
    }
    if held.platform {
        mods |= KeyModifiers::SUPER;
    }
    if let Some(code) = named(key) {
        return Some((code, mods));
    }
    let chord = held.control || held.alt;
    let ch = if chord {
        key.chars().next()
    } else {
        typed.and_then(|s| s.chars().next())
    }?;
    if !chord {
        // The typed character already carries shift.
        mods.remove(KeyModifiers::SHIFT);
    }
    Some((KeyCode::Char(ch), mods))
}

fn named(key: &str) -> Option<KeyCode> {
    Some(match key {
        "enter" => KeyCode::Enter,
        "escape" => KeyCode::Escape,
        "backspace" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "left" => KeyCode::LeftArrow,
        "right" => KeyCode::RightArrow,
        "up" => KeyCode::UpArrow,
        "down" => KeyCode::DownArrow,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        f if f.len() > 1 && f.starts_with('f') => KeyCode::Function(f[1..].parse().ok()?),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Held = Held {
        shift: false,
        control: false,
        alt: false,
        platform: false,
    };

    #[test]
    fn text_named_and_chords() {
        assert_eq!(
            terminal_key(
                "a",
                Some("A"),
                Held {
                    shift: true,
                    ..NONE
                }
            ),
            Some((KeyCode::Char('A'), KeyModifiers::NONE)),
            "the typed character carries shift"
        );
        assert_eq!(
            terminal_key(
                "c",
                Some("c"),
                Held {
                    control: true,
                    ..NONE
                }
            ),
            Some((KeyCode::Char('c'), KeyModifiers::CTRL))
        );
        assert_eq!(
            terminal_key(
                "x",
                None,
                Held {
                    alt: true,
                    shift: true,
                    ..NONE
                }
            ),
            Some((KeyCode::Char('x'), KeyModifiers::ALT | KeyModifiers::SHIFT))
        );
        assert_eq!(
            terminal_key("escape", None, NONE),
            Some((KeyCode::Escape, KeyModifiers::NONE))
        );
        assert_eq!(
            terminal_key(
                "tab",
                None,
                Held {
                    shift: true,
                    ..NONE
                }
            ),
            Some((KeyCode::Tab, KeyModifiers::SHIFT))
        );
        assert_eq!(
            terminal_key("f12", None, NONE),
            Some((KeyCode::Function(12), KeyModifiers::NONE))
        );
        assert_eq!(
            terminal_key("space", Some(" "), NONE),
            Some((KeyCode::Char(' '), KeyModifiers::NONE))
        );
        assert_eq!(
            terminal_key(
                "shift",
                None,
                Held {
                    shift: true,
                    ..NONE
                }
            ),
            None,
            "a bare modifier"
        );
        assert_eq!(terminal_key("fn", None, NONE), None);
    }
}
