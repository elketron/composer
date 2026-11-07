use std::path::PathBuf;

use crate::apps::ProjectName;

/// The project an app was opened on: its name and folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantedProject {
    pub name: ProjectName,
    pub folder: PathBuf,
}
