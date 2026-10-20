use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Harnesses (or harness models) turned off for a project at runtime: `state/projects.json`,
/// `{ "<project folder>": ["claude-code", "claude-code:opus"] }`. It can only narrow what
/// config.toml allows.
pub struct HarnessSwitches {
    file: PathBuf,
    off: Mutex<BTreeMap<String, Vec<String>>>,
}

impl HarnessSwitches {
    pub fn load(file: PathBuf) -> Self {
        let off = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            file,
            off: Mutex::new(off),
        }
    }

    /// Whether `harness` (running `model`) is off for `project`: the harness itself, or that model.
    pub fn is_off(&self, project: &str, harness: &str, model: Option<&str>) -> bool {
        let off = self.off.lock().expect("switches lock");
        off.get(project).is_some_and(|entries| {
            entries
                .iter()
                .any(|e| e == harness || model.is_some_and(|m| *e == format!("{harness}:{m}")))
        })
    }

    /// The entries turned off for `project`.
    pub fn off(&self, project: &str) -> Vec<String> {
        let off = self.off.lock().expect("switches lock");
        off.get(project).cloned().unwrap_or_default()
    }

    /// Turns `entry` (`claude-code` or `claude-code:<model>`) on or off for `project`.
    pub fn set(&self, project: &str, entry: &str, on: bool) -> Result<(), String> {
        let mut off = self.off.lock().expect("switches lock");
        let entries = off.entry(project.to_string()).or_default();
        entries.retain(|e| e != entry);
        if !on {
            entries.push(entry.to_string());
        }
        if entries.is_empty() {
            off.remove(project);
        }
        let json = serde_json::to_string_pretty(&*off).expect("switches serialize");
        std::fs::write(&self.file, json).map_err(|e| format!("{}: {e}", self.file.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_on_and_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("projects.json");
        let switches = HarnessSwitches::load(file.clone());
        assert!(!switches.is_off("/p", "claude-code", None));

        switches.set("/p", "claude-code:opus", false).unwrap();
        assert!(switches.is_off("/p", "claude-code", Some("opus")));
        assert!(!switches.is_off("/p", "claude-code", Some("sonnet")));
        switches.set("/p", "claude-code", false).unwrap();
        assert!(switches.is_off("/p", "claude-code", Some("sonnet")));
        assert!(!switches.is_off("/other", "claude-code", None));

        // Survives a restart.
        let again = HarnessSwitches::load(file);
        assert_eq!(again.off("/p"), ["claude-code:opus", "claude-code"]);
        again.set("/p", "claude-code", true).unwrap();
        again.set("/p", "claude-code:opus", true).unwrap();
        assert!(again.off("/p").is_empty());
    }
}
