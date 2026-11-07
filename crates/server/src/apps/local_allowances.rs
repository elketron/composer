use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::apps::{LocalApp, ProjectName};

/// The project-local apps the user allowed (`composer app allow <project> <app>`), each for the
/// exact `index.md` and `justfile` it had then: a changed one is off again until allowed again.
/// In the workspace's `state/local_apps.json`.
pub struct LocalAllowances {
    path: PathBuf,
    allowed: Mutex<Vec<Allowance>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Allowance {
    project: String,
    app: String,
    hash: String,
    allowed_at: String,
}

impl LocalAllowances {
    pub fn load(path: PathBuf) -> Self {
        let allowed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            allowed: Mutex::new(allowed),
        }
    }

    /// `app` of `project` is allowed as it is now.
    pub fn allows(&self, project: &ProjectName, app: &LocalApp) -> bool {
        self.allowed
            .lock()
            .expect("allowances lock")
            .iter()
            .any(|a| a.project == project.as_str() && a.app == app.name && a.hash == app.hash)
    }

    /// Allows `app` of `project` as it is now (an older allowance of it is replaced).
    pub fn allow(&self, project: &ProjectName, app: &LocalApp) -> Result<(), String> {
        let mut allowed = self.allowed.lock().expect("allowances lock");
        allowed.retain(|a| !(a.project == project.as_str() && a.app == app.name));
        allowed.push(Allowance {
            project: project.to_string(),
            app: app.name.clone(),
            hash: app.hash.clone(),
            allowed_at: chrono::Utc::now().to_rfc3339(),
        });
        let json = serde_json::to_string_pretty(&*allowed).expect("allowances serialize");
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&self.path, json).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}
