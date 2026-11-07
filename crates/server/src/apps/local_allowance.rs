use serde::{Deserialize, Serialize};

/// One allowance: `app` of `project`, for the `hash` of its `index.md` and `justfile`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Allowance {
    pub project: String,
    pub app: String,
    pub hash: String,
    pub allowed_at: String,
}
