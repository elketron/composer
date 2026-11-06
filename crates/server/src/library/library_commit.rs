use std::fmt;

/// A commit of the library repo: a full 40-hex-digit SHA-1, as git prints it. Only built from
/// git's own output or checked input, so it can always be put in a git command line (never an
/// option, never a range).
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct LibraryCommit(String);

impl LibraryCommit {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.len() == 40 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(Self(raw.to_ascii_lowercase()))
        } else {
            Err(format!("`{raw}` is not a library commit (40 hex digits)"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first 12 digits, for messages.
    pub fn short(&self) -> &str {
        &self.0[..12]
    }
}

impl TryFrom<String> for LibraryCommit {
    type Error = String;
    fn try_from(raw: String) -> Result<Self, String> {
        Self::parse(&raw)
    }
}

impl From<LibraryCommit> for String {
    fn from(commit: LibraryCommit) -> String {
        commit.0
    }
}

impl fmt::Display for LibraryCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_full_hashes() {
        let sha = "0123456789abcdef0123456789ABCDEF01234567";
        let commit = LibraryCommit::parse(sha).unwrap();
        assert_eq!(commit.as_str(), sha.to_ascii_lowercase());
        assert_eq!(commit.short(), "0123456789ab");
        for bad in [
            "",
            "HEAD",
            "abc123",
            "--all",
            &format!("{sha}0"),
            &"g".repeat(40),
        ] {
            assert!(LibraryCommit::parse(bad).is_err(), "{bad}");
        }
    }
}
