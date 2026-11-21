use std::path::PathBuf;

/// What setting a site found in its `folder:`, for the note that comes with it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderNote {
    /// The folder it names, canonical; `None` when it names none, or one that does not exist.
    pub folder: Option<PathBuf>,
    /// The user approved that folder for the site.
    pub approved: bool,
}

impl FolderNote {
    /// Words about it: whether the folder counts. Empty when the site names none.
    pub fn describe(&self) -> String {
        let Some(folder) = &self.folder else {
            return String::new();
        };
        let folder = crate::project::tilde(folder);
        if self.approved {
            format!("Its folder is {folder}.")
        } else {
            format!(
                "Note: its folder {folder} is not approved, so nothing works there: terminals \
                 and processes start elsewhere, and there is no changes view."
            )
        }
    }
}
