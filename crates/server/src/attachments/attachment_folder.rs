use std::path::PathBuf;

use crate::session::SessionFolder;
use crate::workspace_home::WorkspaceHome;

/// A session's attachments folder in one workspace: `state/sessions/<id>/attachments`.
///
/// Built only from a session id that is one plain name (letters, digits, `-`, `_`), so an id from
/// a URL, a ticket or the model's history (`..`, `../../../b/state/sessions/x`) can never name a
/// folder outside this workspace's sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentFolder {
    session_dir: PathBuf,
}

impl AttachmentFolder {
    /// `session`'s folder in `home`; refused unless `session` is a plain session id.
    pub fn of(home: &WorkspaceHome, session: &str) -> Result<Self, String> {
        let folder = SessionFolder::of(&home.sessions(), session)?;
        Ok(Self {
            session_dir: folder.path().to_path_buf(),
        })
    }

    /// Where the session's attachments are stored.
    pub fn path(&self) -> PathBuf {
        self.session_dir.join("attachments")
    }

    /// The session's `meta.json` (its project, for a ticket's attachments).
    pub fn session_meta(&self) -> PathBuf {
        self.session_dir.join("meta.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn only_a_plain_session_id_names_a_folder() {
        let home = WorkspaceHome::new("/h");
        let folder = AttachmentFolder::of(&home, "a1b2-w_3").unwrap();
        assert_eq!(
            folder.path(),
            Path::new("/h/state/sessions/a1b2-w_3/attachments")
        );
        for bad in ["", "..", ".", "../b", "a/b", "a\\b", "x%2F", "~"] {
            assert!(AttachmentFolder::of(&home, bad).is_err(), "{bad}");
        }
    }
}
