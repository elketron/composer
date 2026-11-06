use std::fmt;
use std::str::FromStr;

use crate::error::{Result, ServerError};

/// A workspace's name: its folder under `workspaces/` and, later, its route segment `/w/<name>/`.
///
/// Only a valid name can exist: 1 to 64 characters of `a-z`, `0-9`, `-` and `_`, starting with a
/// letter or digit, and not one of the folder names the home itself uses. So a name can be joined
/// onto a path or a URL without escaping, and can never reach outside `workspaces/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkspaceName(String);

impl WorkspaceName {
    /// The workspace `init` creates and `migrate` moves an old home into.
    pub const DEFAULT: &'static str = "default";

    /// Names the home layout or the routes use for themselves.
    const RESERVED: [&'static str; 8] = [
        "global",
        "sites",
        "state",
        "templates",
        "workspaces",
        "w",
        "api",
        "ws",
    ];

    pub fn new(name: &str) -> Result<Self> {
        let valid_chars = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        let starts_well = name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        if name.len() > 64 || !valid_chars || !starts_well {
            return Err(ServerError::Invalid(format!(
                "`{name}` is not a valid workspace name: use 1-64 characters of a-z, 0-9, `-` and \
                 `_`, starting with a letter or digit (e.g. `3d-printing`)"
            )));
        }
        if Self::RESERVED.contains(&name) {
            return Err(ServerError::Invalid(format!(
                "`{name}` is reserved by the composer home layout; pick another workspace name"
            )));
        }
        Ok(Self(name.to_string()))
    }

    /// The `default` workspace.
    pub fn default_workspace() -> Self {
        Self(Self::DEFAULT.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WorkspaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for WorkspaceName {
    type Err = ServerError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_names() {
        for name in ["default", "3d-printing", "client_x", "a"] {
            assert_eq!(WorkspaceName::new(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn refuses_what_could_escape_or_collide() {
        for name in [
            "",
            "..",
            "a/b",
            "a\\b",
            "-x",
            "_x",
            "Caps",
            "sp ace",
            "global",
            "sites",
            "state",
            "templates",
            "workspaces",
            "w",
            "api",
            "ws",
        ] {
            assert!(WorkspaceName::new(name).is_err(), "{name:?} was accepted");
        }
        assert!(WorkspaceName::new(&"a".repeat(65)).is_err());
        assert!(WorkspaceName::new(&"a".repeat(64)).is_ok());
    }
}
