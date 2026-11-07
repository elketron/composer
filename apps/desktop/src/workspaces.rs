//! The home's workspaces as the desktop sees them: their names (folders with a token, i.e. the
//! server has run them) and their colours.

use std::path::Path;

/// The workspaces of the home at `home`, sorted (`default` first).
pub fn list(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(home.join("workspaces"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("state/token").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    names.sort_by_key(|n| (n != "default", n.clone()));
    names
}

/// A workspace's colour: `[desktop] color = "#rrggbb"` in its config, else one from its name.
pub fn colour(home: &Path, name: &str) -> u32 {
    std::fs::read_to_string(home.join("workspaces").join(name).join("config.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|t| t.get("desktop")?.get("color")?.as_str().map(str::to_string))
        .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
        .unwrap_or_else(|| from_name(name))
}

/// Distinct, readable on dark: a hue from the name, fixed saturation and lightness.
pub fn from_name(name: &str) -> u32 {
    let hash = name
        .bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let hue = (hash % 360) as f32;
    hsl_to_rgb(hue, 0.55, 0.62)
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
    fn listed_and_coloured() {
        let home = tempfile::tempdir().unwrap();
        for (name, token) in [
            ("work", true),
            ("default", true),
            ("aaa", true),
            ("new", false),
        ] {
            let dir = home.path().join("workspaces").join(name).join("state");
            std::fs::create_dir_all(&dir).unwrap();
            if token {
                std::fs::write(dir.join("token"), "t").unwrap();
            }
        }
        assert_eq!(
            list(home.path()),
            ["default", "aaa", "work"],
            "default first; not yet run: left out"
        );
        assert_eq!(from_name("work"), from_name("work"), "stable");
        assert_ne!(from_name("work"), from_name("default"));
        std::fs::write(
            home.path().join("workspaces/work/config.toml"),
            "[desktop]\ncolor = \"#ff8800\"\n",
        )
        .unwrap();
        assert_eq!(colour(home.path(), "work"), 0xff8800);
        assert_eq!(colour(home.path(), "aaa"), from_name("aaa"));
        assert_eq!(hsl_to_rgb(0.0, 1.0, 0.5), 0xff0000);
        assert_eq!(hsl_to_rgb(120.0, 1.0, 0.5), 0x00ff00);
    }
}
