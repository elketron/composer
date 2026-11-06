use std::path::{Path, PathBuf};

/// One session's folder, `<sessions>/<id>`: built only from an id that is one plain name
/// (letters, digits, `-`, `_`), so no id from a URL, a ticket, a worker's command line or the
/// model's history (`..`, `../../../b/state/sessions/x`) names a folder outside its workspace's
/// sessions (tickets 0148, 0150).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFolder(PathBuf);

impl SessionFolder {
    /// `id`'s folder in the sessions folder `sessions`; refused unless `id` is a plain id.
    pub fn of(sessions: &Path, id: &str) -> Result<Self, String> {
        let plain = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !plain {
            return Err(format!(
                "`{id}` is not a session id (letters, digits, - and _)"
            ));
        }
        Ok(Self(sessions.join(id)))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_id_names_a_session_folder() {
        let folder = SessionFolder::of(Path::new("/s"), "a1b2-w_3").unwrap();
        assert_eq!(folder.path(), Path::new("/s/a1b2-w_3"));
        for bad in ["", "..", ".", "../b", "a/b", "a\\b", "x%2F", "~", "a b"] {
            assert!(SessionFolder::of(Path::new("/s"), bad).is_err(), "{bad}");
        }
    }
}
