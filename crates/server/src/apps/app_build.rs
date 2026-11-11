use std::path::Path;

/// The app sdk's version now (`SDK_VERSION` in `templates/sdk/client.svelte.ts`). A build with an
/// older one (or none: before builds were stamped) doesn't know the sandboxed `/a/<app token>/`
/// routes (3) and shows nothing there: it is refused with a page saying to rebuild it.
pub const SDK_VERSION: u32 = 3;

/// The sdk version `dir`'s app was built with (`dir/ui/dist`): `None` when it isn't built, `0`
/// when the build has no stamp.
pub fn built_with(dir: &Path) -> Option<u32> {
    let dist = dir.join("ui/dist");
    if !dist.join("index.html").is_file() {
        return None;
    }
    let scripts = [dist.clone(), dist.join("assets")]
        .into_iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "js"));
    let stamps = scripts
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|text| stamp(&text));
    Some(stamps.max().unwrap_or(0))
}

/// The app in `dir` is built, with an sdk older than this one.
pub fn outdated(dir: &Path) -> bool {
    built_with(dir).is_some_and(|v| v < SDK_VERSION)
}

/// The highest `composer-sdk:<n>` in a script.
fn stamp(text: &str) -> Option<u32> {
    text.match_indices("composer-sdk:")
        .filter_map(|(at, marker)| {
            let digits: String = text[at + marker.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_says_its_sdk() {
        let app = tempfile::tempdir().unwrap();
        assert_eq!(built_with(app.path()), None, "not built");
        let assets = app.path().join("ui/dist/assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(app.path().join("ui/dist/index.html"), "<p>").unwrap();
        std::fs::write(assets.join("a.js"), "let x=1;").unwrap();
        assert_eq!(built_with(app.path()), Some(0));
        assert!(outdated(app.path()));
        std::fs::write(assets.join("a.js"), "d.composerSdk=\"composer-sdk:2\"").unwrap();
        assert_eq!(built_with(app.path()), Some(2));
        assert!(outdated(app.path()), "2 doesn't know the sandboxed routes");
        std::fs::write(assets.join("a.js"), "d.composerSdk=\"composer-sdk:3\"").unwrap();
        assert!(!outdated(app.path()));
    }
}
