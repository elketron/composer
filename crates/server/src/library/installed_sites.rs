use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::library::{Installed, SiteName};

/// A workspace's installed sites (`state/library.json`): its site name → where it came from.
/// Kept in state, not in a page, so no agent edits it. Names and commits are checked when read: an
/// entry that isn't a site name or a full commit makes the whole file unreadable, not half-trusted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstalledSites {
    file: PathBuf,
    sites: BTreeMap<SiteName, Installed>,
}

impl InstalledSites {
    /// The record at `file`; empty when there is none yet.
    pub fn load(file: &Path) -> Result<Self, String> {
        let sites = match std::fs::read_to_string(file) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} is not a list of installed sites: {e}", file.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(format!("{}: {e}", file.display())),
        };
        Ok(Self {
            file: file.to_path_buf(),
            sites,
        })
    }

    pub fn get(&self, site: &SiteName) -> Option<&Installed> {
        self.sites.get(site)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SiteName, &Installed)> {
        self.sites.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Records `site` as installed from `installed`, and saves.
    pub fn set(&mut self, site: SiteName, installed: Installed) -> Result<(), String> {
        self.sites.insert(site, installed);
        self.save()
    }

    /// Saves the record as it is (also an empty one: the workspace installs from the library).
    pub fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.sites).expect("installed sites serialize");
        crate::init::write_creating_dirs(&self.file, text.as_bytes()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LibraryCommit;

    #[test]
    fn saved_and_checked_when_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("state/library.json");
        let mut sites = InstalledSites::load(&file).unwrap();
        assert!(sites.is_empty());
        let commit = LibraryCommit::parse(&"a".repeat(40)).unwrap();
        let cad = SiteName::new("cad").unwrap();
        sites
            .set(
                SiteName::new("my-cad").unwrap(),
                Installed {
                    from: cad.clone(),
                    commit: commit.clone(),
                },
            )
            .unwrap();
        let again = InstalledSites::load(&file).unwrap();
        assert_eq!(
            again.get(&SiteName::new("my-cad").unwrap()).unwrap().from,
            cad
        );

        std::fs::write(&file, r#"{"../x": {"from": "cad", "commit": "HEAD"}}"#).unwrap();
        assert!(InstalledSites::load(&file).is_err());
    }
}
