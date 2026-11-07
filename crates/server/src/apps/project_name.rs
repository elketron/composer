use std::fmt;
use std::path::Path;

/// What a project is called in its workspace and in URLs (`/w/<ws>/p/<name>/<app>/`): its folder's
/// name, or the `name` of its `[projects."<path>"]` entry. One name per project in a workspace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectName(String);

impl ProjectName {
    pub fn new(name: &str) -> Result<Self, String> {
        let ok = !name.is_empty()
            && !name.starts_with('.')
            && name.len() <= 100
            && !name
                .chars()
                .any(|c| c == '/' || c == '\\' || c.is_control());
        if !ok {
            return Err(format!(
                "`{name}` isn't a project name: a folder name (no `/`, not starting with `.`)"
            ));
        }
        Ok(Self(name.to_string()))
    }

    /// The name of the project in `folder` (its last component).
    pub fn of_folder(folder: &Path) -> Result<Self, String> {
        let name = folder
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Self::new(&name)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_only() {
        assert_eq!(
            ProjectName::of_folder(Path::new("/home/u/Projects/composer"))
                .unwrap()
                .as_str(),
            "composer"
        );
        assert!(ProjectName::new("my game").is_ok());
        for bad in ["", ".hidden", "a/b", "a\\b", "x\ny"] {
            assert!(ProjectName::new(bad).is_err(), "{bad:?}");
        }
    }
}
