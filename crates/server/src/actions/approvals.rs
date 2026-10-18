use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::actions::Approval;
use crate::error::{Result, ServerError};

/// Approved recipe versions, persisted in `state/approvals.json` (outside every site,
/// so agents cannot approve their own recipes).
#[derive(Debug)]
pub struct Approvals {
    path: PathBuf,
    approved: Mutex<Vec<Approval>>,
    /// Worker processes: the parent owns the file; approvals are kept in memory only.
    read_only: bool,
}

impl Approvals {
    pub fn load(path: &Path) -> Result<Self> {
        let approved = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| ServerError::Invalid(format!("{}: {e}", path.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(ServerError::io(path, e)),
        };
        Ok(Self {
            path: path.to_path_buf(),
            approved: Mutex::new(approved),
            read_only: false,
        })
    }

    /// For worker processes: reads the file, never writes it.
    pub fn load_read_only(path: &Path) -> Result<Self> {
        Ok(Self {
            read_only: true,
            ..Self::load(path)?
        })
    }

    pub fn is_approved(&self, site: &str, recipe: &str, hash: &str) -> bool {
        self.approved
            .lock()
            .expect("approvals lock")
            .iter()
            .any(|a| a.site == site && a.recipe == recipe && a.hash == hash)
    }

    /// Records an approval. Older versions of the same recipe are forgotten.
    pub fn approve(&self, site: &str, recipe: &str, hash: &str) -> Result<()> {
        let mut approved = self.approved.lock().expect("approvals lock");
        approved.retain(|a| !(a.site == site && a.recipe == recipe));
        approved.push(Approval {
            site: site.to_string(),
            recipe: recipe.to_string(),
            hash: hash.to_string(),
            approved_at: chrono::Utc::now().to_rfc3339(),
        });
        if self.read_only {
            return Ok(());
        }
        let json = serde_json::to_string_pretty(&*approved).expect("approvals serialize");
        std::fs::write(&self.path, json).map_err(|e| ServerError::io(&self.path, e))
    }
}
