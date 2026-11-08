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

/// A workspace's colour: `[desktop] color = "#rrggbb"` in its config, else one from its name,
/// exactly as `/api/info` reports it (the phone draws the same value).
pub fn colour(home: &Path, name: &str) -> u32 {
    let configured =
        std::fs::read_to_string(home.join("workspaces").join(name).join("config.toml"))
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .and_then(|t| t.get("desktop")?.get("color")?.as_str().map(str::to_string))
            .unwrap_or_default();
    composer_protocol::WorkspaceColor::for_workspace(name, &configured).rgb()
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
        let from_name = composer_protocol::WorkspaceColor::from_name;
        assert_eq!(from_name("work"), from_name("work"), "stable");
        assert_ne!(from_name("work"), from_name("default"));
        std::fs::write(
            home.path().join("workspaces/work/config.toml"),
            "[desktop]\ncolor = \"#ff8800\"\n",
        )
        .unwrap();
        assert_eq!(colour(home.path(), "work"), 0xff8800);
        assert_eq!(colour(home.path(), "aaa"), from_name("aaa").rgb());
    }
}
