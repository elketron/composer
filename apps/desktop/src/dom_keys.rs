//! Keys for Chromium: the Windows virtual key code its offscreen key path keys off, and the native
//! (XKB) key code it derives `KeyboardEvent.code` from, for the key's place on a US layout (gpui
//! gives the key, not the scancode).

/// (windows key code, native key code) for a gpui key name.
pub fn codes(key: &str) -> Option<(i32, i32)> {
    let windows = windows_code(key)?;
    Some((windows, native_code(key).unwrap_or(0)))
}

fn windows_code(key: &str) -> Option<i32> {
    Some(match key {
        "enter" => 0x0D,
        "backspace" => 0x08,
        "tab" => 0x09,
        "escape" => 0x1B,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" => 0x2D,
        "delete" => 0x2E,
        k if k.chars().count() == 1 => {
            let c = k.chars().next()?.to_ascii_uppercase();
            if c.is_ascii_alphanumeric() {
                c as i32
            } else {
                punct_code(c)?
            }
        }
        k if k.len() > 1 && k.starts_with('f') => 0x6F + k[1..].parse::<i32>().ok()?,
        _ => return None,
    })
}

fn punct_code(c: char) -> Option<i32> {
    Some(match c {
        ';' => 0xBA,
        '=' => 0xBB,
        ',' => 0xBC,
        '-' => 0xBD,
        '.' => 0xBE,
        '/' => 0xBF,
        '`' => 0xC0,
        '[' => 0xDB,
        '\\' => 0xDC,
        ']' => 0xDD,
        '\'' => 0xDE,
        _ => return None,
    })
}

/// The X keycode (evdev + 8) of the key's place on a US keyboard.
fn native_code(key: &str) -> Option<i32> {
    const ROWS: [(&str, i32); 4] = [
        ("1234567890-=", 10),
        ("qwertyuiop[]", 24),
        ("asdfghjkl;'", 38),
        ("zxcvbnm,./", 52),
    ];
    let evdev = match key {
        "escape" => 1 + 8,
        "backspace" => 22,
        "tab" => 23,
        "enter" => 36,
        "space" => 65,
        "`" => 49,
        "\\" => 51,
        "home" => 110,
        "up" => 111,
        "pageup" => 112,
        "left" => 113,
        "right" => 114,
        "end" => 115,
        "down" => 116,
        "pagedown" => 117,
        "insert" => 118,
        "delete" => 119,
        k if k.len() > 1 && k.starts_with('f') => match k[1..].parse::<i32>().ok()? {
            n @ 1..=10 => 66 + n,
            11 => 95,
            12 => 96,
            _ => return None,
        },
        k => {
            let c = k
                .chars()
                .next()
                .filter(|_| k.chars().count() == 1)?
                .to_ascii_lowercase();
            return ROWS
                .iter()
                .find_map(|(row, first)| row.find(c).map(|i| first + i as i32));
        }
    };
    Some(evdev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_named() {
        assert_eq!(codes("a"), Some((0x41, 38)));
        assert_eq!(codes("Q"), Some((0x51, 24)));
        assert_eq!(codes("1"), Some((0x31, 10)));
        assert_eq!(codes("0"), Some((0x30, 19)));
        assert_eq!(codes("enter"), Some((0x0D, 36)));
        assert_eq!(codes("escape"), Some((0x1B, 9)));
        assert_eq!(codes("left"), Some((0x25, 113)));
        assert_eq!(codes("f1"), Some((0x70, 67)));
        assert_eq!(codes("f12"), Some((0x7B, 96)));
        assert_eq!(codes("/"), Some((0xBF, 61)));
        assert_eq!(codes("é"), None, "no windows code: not sent as a raw key");
        assert_eq!(codes("shift"), None);
    }
}
