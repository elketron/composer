//! A workspace's colour: `[desktop] color` in its config, else one derived from its name.
//!
//! The server reports it in `/api/info`; the desktop and the phone draw it. One implementation so
//! they can never disagree.

/// A colour as `#rrggbb`. Constructors validate, so an invalid colour cannot exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceColor(u32);

impl WorkspaceColor {
    /// `#rrggbb` (the `#` is optional). `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16).ok().map(Self)
    }

    /// A distinct, readable-on-dark hue from the name: fixed saturation and lightness.
    pub fn from_name(name: &str) -> Self {
        let hash = name
            .bytes()
            .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        let hue = (hash % 360) as f32;
        Self(hsl_to_rgb(hue, 0.55, 0.62))
    }

    /// The configured colour if it parses, else one from the name.
    pub fn for_workspace(name: &str, configured: &str) -> Self {
        Self::parse(configured).unwrap_or_else(|| Self::from_name(name))
    }

    pub fn rgb(self) -> u32 {
        self.0
    }

    pub fn to_hex(self) -> String {
        format!("#{:06x}", self.0)
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> u32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| ((v + m) * 255.0).round() as u32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        assert_eq!(WorkspaceColor::parse("#ff8800").unwrap().rgb(), 0xff8800);
        assert_eq!(WorkspaceColor::parse("ff8800").unwrap().to_hex(), "#ff8800");
        assert!(WorkspaceColor::parse("#fff").is_none());
        assert!(WorkspaceColor::parse("#gggggg").is_none());
        assert!(WorkspaceColor::parse("").is_none());
    }

    #[test]
    fn configured_wins_else_from_name() {
        assert_eq!(
            WorkspaceColor::for_workspace("work", "#123456").to_hex(),
            "#123456"
        );
        let named = WorkspaceColor::for_workspace("work", "not a colour");
        assert_eq!(named, WorkspaceColor::from_name("work"), "stable");
        assert_ne!(
            WorkspaceColor::from_name("work"),
            WorkspaceColor::from_name("default")
        );
        assert_eq!(hsl_to_rgb(0.0, 1.0, 0.5), 0xff0000);
        assert_eq!(hsl_to_rgb(120.0, 1.0, 0.5), 0x00ff00);
    }
}
