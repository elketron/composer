use std::fmt;
use std::path::Path;

/// A file pattern of an app's manifest (`*.FCStd`, `docs/**/*.md`): `*` within a name, `**` for
/// any folders. Without a `/` it matches a file's name wherever it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGlob(String);

impl FileGlob {
    pub fn new(pattern: &str) -> Result<Self, String> {
        let pattern = pattern.trim();
        if pattern.is_empty() || pattern.starts_with('/') || pattern.split('/').any(|p| p == "..") {
            return Err(format!(
                "`{pattern}` isn't a file pattern: give one relative to the project, like \
                 `*.FCStd` or `docs/**/*.md`"
            ));
        }
        Ok(Self(pattern.to_string()))
    }

    /// Whether `file` (relative to the project) matches.
    pub fn matches(&self, file: &Path) -> bool {
        let path = file.to_string_lossy();
        if !self.0.contains('/') {
            let name = file.file_name().map(|n| n.to_string_lossy());
            return name.is_some_and(|name| segment_matches(&self.0, &name));
        }
        let pattern: Vec<&str> = self.0.split('/').collect();
        let parts: Vec<&str> = path.split('/').collect();
        path_matches(&pattern, &parts)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FileGlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `**` takes any number of folders; every other piece matches one name.
fn path_matches(pattern: &[&str], parts: &[&str]) -> bool {
    match (pattern.split_first(), parts.split_first()) {
        (None, None) => true,
        (Some((&"**", rest)), _) => {
            path_matches(rest, parts) || (!parts.is_empty() && path_matches(pattern, &parts[1..]))
        }
        (Some((piece, rest)), Some((part, more))) => {
            segment_matches(piece, part) && path_matches(rest, more)
        }
        _ => false,
    }
}

/// `*` for any run of characters within one name.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let Some((first, rest)) = pattern.split_once('*') else {
        return pattern == name;
    };
    let Some(mut left) = name.strip_prefix(first) else {
        return false;
    };
    let pieces: Vec<&str> = rest.split('*').collect();
    let (last, middle) = pieces.split_last().expect("split gives at least one piece");
    for piece in middle {
        let Some(at) = left.find(piece) else {
            return false;
        };
        left = &left[at + piece.len()..];
    }
    left.len() >= last.len() && left.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glob(p: &str) -> FileGlob {
        FileGlob::new(p).unwrap()
    }

    #[test]
    fn names_anywhere_and_paths_with_double_stars() {
        assert!(glob("*.FCStd").matches(Path::new("parts/arm.FCStd")));
        assert!(glob("*.FCStd").matches(Path::new("arm.FCStd")));
        assert!(!glob("*.FCStd").matches(Path::new("arm.step")));
        assert!(glob("docs/**/*.md").matches(Path::new("docs/a.md")));
        assert!(glob("docs/**/*.md").matches(Path::new("docs/x/y/a.md")));
        assert!(!glob("docs/**/*.md").matches(Path::new("src/a.md")));
        assert!(glob("Cargo.toml").matches(Path::new("crates/x/Cargo.toml")));
        assert!(
            glob("a*b*c").matches(Path::new("aXbYc")) && !glob("a*b*c").matches(Path::new("acb"))
        );
        assert!(!glob("ab*ba").matches(Path::new("aba")));
    }

    #[test]
    fn refuses_what_isnt_a_pattern() {
        for bad in ["", "/etc/*", "../x", "a/../b"] {
            assert!(FileGlob::new(bad).is_err(), "{bad}");
        }
    }
}
