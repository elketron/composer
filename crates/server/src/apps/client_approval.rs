use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One approval of a client app: `site`'s built `ui/dist` as it was then, file by file (its path
/// in `dist` → the sha256 of its bytes), and one hash of all of it for messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClientApproval {
    pub site: String,
    pub hash: String,
    pub files: BTreeMap<String, String>,
    pub approved_at: String,
}
