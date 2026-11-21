use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tempfile::NamedTempFile;

use crate::actions::Approval;
use crate::error::{Result, ServerError};

/// Approved recipe versions, persisted in `state/approvals.json` (outside every site,
/// so agents cannot approve their own recipes).
#[derive(Debug)]
pub struct Approvals {
    path: PathBuf,
    approved: Mutex<Vec<Approval>>,
}

impl Approvals {
    pub fn load(path: &Path) -> Result<Self> {
        let approved: Vec<Approval> = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| ServerError::Invalid(format!("{}: {e}", path.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(ServerError::io(path, e)),
        };
        Ok(Self {
            path: path.to_path_buf(),
            // What apps were allowed never counts for the agent.
            approved: Mutex::new(
                approved
                    .into_iter()
                    .filter(|a| a.caller.is_none())
                    .collect(),
            ),
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
            caller: None,
            approved_at: chrono::Utc::now().to_rfc3339(),
        });
        let json = serde_json::to_string_pretty(&*approved).expect("approvals serialize");
        self.replace_file(&json)
            .map_err(|e| ServerError::io(&self.path, e))
    }

    /// Writes a temporary file and renames it, so that a crash or a process starting meanwhile
    /// never finds half a file: `load` refuses one it can't parse.
    fn replace_file(&self, json: &str) -> std::io::Result<()> {
        let mut file = NamedTempFile::new_in(self.path.parent().unwrap_or(Path::new(".")))?;
        file.write_all(json.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&self.path)?;
        Ok(())
    }
}
