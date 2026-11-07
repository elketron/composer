use std::path::{Path, PathBuf};

use crate::home::Home;

/// Claude Code's login for sandboxed runs: a long-lived token from `claude setup-token`, kept in
/// the home's `state/claude-token` (owner-only; no sandbox mounts it) and given to each run as
/// `CLAUDE_CODE_OAUTH_TOKEN`, so no credentials file is shared with a sandbox.
#[derive(Clone, PartialEq, Eq)]
pub struct ClaudeToken(String);

impl ClaudeToken {
    /// The env variable Claude Code reads it from.
    pub const ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

    /// `raw` as a token: one word of printable characters, long enough to be one.
    pub fn new(raw: &str) -> Result<Self, String> {
        let token = raw.trim();
        let plain = token.len() >= 20 && token.chars().all(|c| c.is_ascii_graphic());
        if !plain {
            return Err(
                "that isn't a Claude Code token: paste the one `claude setup-token` prints".into(),
            );
        }
        Ok(Self(token.to_string()))
    }

    /// Where `home` keeps it.
    pub fn file(home: &Home) -> PathBuf {
        home.state().join("claude-token")
    }

    /// The stored token; `None` when there is none yet (or it doesn't read as one).
    pub fn load(home: &Home) -> Option<Self> {
        let text = std::fs::read_to_string(Self::file(home)).ok()?;
        Self::new(&text).ok()
    }

    /// Stores it for `home`, readable by the owner only.
    pub fn save(&self, home: &Home) -> Result<PathBuf, String> {
        let file = Self::file(home);
        write_private(&file, &self.0).map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(file)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// What a run is told when there is none.
    pub fn missing() -> String {
        "Claude Code runs in the sandbox, which needs its own login: run `claude setup-token`, \
         then `composer claude-token` and paste the token it printed"
            .into()
    }
}

/// Never printed in logs or debug output.
impl std::fmt::Debug for ClaudeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClaudeToken(…)")
    }
}

fn write_private(file: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(file)?;
    out.write_all(text.as_bytes())?;
    out.write_all(b"\n")?;
    // An existing file keeps its mode on open: make sure.
    std::fs::set_permissions(file, std::os::unix::fs::PermissionsExt::from_mode(0o600))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_owner_only_and_never_shown() {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::new(dir.path());
        assert!(ClaudeToken::load(&home).is_none());
        assert!(ClaudeToken::new("short").is_err());
        assert!(ClaudeToken::new("has a space in it, so no").is_err());
        let token = ClaudeToken::new("  sk-ant-oat01-abcdefghijklmnop\n").unwrap();
        let file = token.save(&home).unwrap();
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&file).unwrap().permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(
            ClaudeToken::load(&home).unwrap().as_str(),
            "sk-ant-oat01-abcdefghijklmnop"
        );
        assert!(!format!("{token:?}").contains("sk-ant"));
    }
}
