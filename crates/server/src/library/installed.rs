use serde::{Deserialize, Serialize};

use crate::library::{LibraryCommit, SiteName};

/// Where one installed site came from: a library site at a commit (the base of its next update).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub from: SiteName,
    pub commit: LibraryCommit,
}
