use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::apps::client_approval::ClientApproval;

/// Client apps (`[apps] clients`: the chat app) run with the workspace's credentials, unsandboxed,
/// but their `ui/dist` is files an agent can write. So one is served only as the user approved it
/// (`composer app approve <site>`): every file it serves must be one of the approved build, with
/// the same bytes; a changed build is refused until approved again. In the workspace's
/// `state/client_apps.json`, read on every check, so the CLI writes it with the server running.
/// Nothing an agent or an app can reach approves: only the user's command does.
pub struct ClientApprovals {
    path: PathBuf,
}

/// Files in a `dist` beyond these many are not taken (no build has that many).
const MAX_FILES: usize = 10_000;

impl ClientApprovals {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Vec<ClientApproval> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn of(&self, site: &str) -> Option<ClientApproval> {
        self.load().into_iter().find(|a| a.site == site)
    }

    /// Approves `site`'s build in `dist` as it is now (an older approval of it is replaced): the
    /// hash approved and the number of files.
    pub fn approve(&self, site: &str, dist: &Path) -> Result<(String, usize), String> {
        if !dist.join("index.html").is_file() {
            return Err(format!(
                "site://{site} isn't built: no {}/index.html (build it with `just ui-build` in the \
                 site first)",
                dist.display()
            ));
        }
        let files = digest(dist)?;
        let hash = whole(&files);
        let mut approvals = self.load();
        approvals.retain(|a| a.site != site);
        let count = files.len();
        approvals.push(ClientApproval {
            site: site.to_string(),
            hash: hash.clone(),
            files,
            approved_at: chrono::Utc::now().to_rfc3339(),
        });
        let json = serde_json::to_string_pretty(&approvals).expect("approvals serialize");
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&self.path, json).map_err(|e| format!("{}: {e}", self.path.display()))?;
        Ok((hash, count))
    }

    /// `site`'s whole build in `dist` is the approved one (for its page: a file added, removed or
    /// changed anywhere says so there).
    pub fn allows(&self, site: &str, dist: &Path) -> bool {
        self.of(site)
            .is_some_and(|a| digest(dist).is_ok_and(|files| files == a.files))
    }

    /// `bytes` are the approved ones of `site`'s file `relative` (a path in its `dist`).
    pub fn allows_file(&self, site: &str, relative: &str, bytes: &[u8]) -> bool {
        self.of(site)
            .is_some_and(|a| a.files.get(relative) == Some(&sha256(bytes)))
    }
}

/// Every file under `dist` (as the server would read it: links followed) → the sha256 of its
/// bytes, by its path in `dist` with `/`.
fn digest(dist: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut files = BTreeMap::new();
    let mut folders = vec![(dist.to_path_buf(), 0)];
    while let Some((folder, depth)) = folders.pop() {
        let entries =
            std::fs::read_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if meta.is_dir() && depth < 16 {
                folders.push((path, depth + 1));
            } else if meta.is_file() {
                let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                let relative = path
                    .strip_prefix(dist)
                    .map_err(|_| format!("{} is outside {}", path.display(), dist.display()))?;
                files.insert(relative_path(relative), sha256(&bytes));
                if files.len() > MAX_FILES {
                    return Err(format!(
                        "{} has more than {MAX_FILES} files",
                        dist.display()
                    ));
                }
            }
        }
    }
    Ok(files)
}

/// `relative` with `/` between its parts: how an approval names a file.
pub fn relative_path(relative: &Path) -> String {
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn whole(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (path, hash) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update([0]);
    }
    hex(&hasher.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_is_served_as_approved_only() {
        let home = tempfile::tempdir().unwrap();
        let dist = home.path().join("chat/ui/dist");
        std::fs::create_dir_all(dist.join("assets")).unwrap();
        std::fs::write(dist.join("index.html"), "<p>chat").unwrap();
        std::fs::write(dist.join("assets/a.js"), "let a = 1;").unwrap();
        let approvals = ClientApprovals::new(home.path().join("state/client_apps.json"));
        assert!(!approvals.allows("chat", &dist), "not approved yet");

        let (_, count) = approvals.approve("chat", &dist).unwrap();
        assert_eq!(count, 2);
        assert!(approvals.allows("chat", &dist));
        assert!(approvals.allows_file("chat", "assets/a.js", b"let a = 1;"));
        assert!(!approvals.allows_file("chat", "assets/a.js", b"steal()"));
        assert!(!approvals.allows_file("other", "assets/a.js", b"let a = 1;"));

        // A file changed or added: no longer the approved build.
        std::fs::write(dist.join("assets/b.js"), "steal()").unwrap();
        assert!(!approvals.allows("chat", &dist));
        assert!(!approvals.allows_file("chat", "assets/b.js", b"steal()"));
        approvals.approve("chat", &dist).unwrap();
        assert!(approvals.allows("chat", &dist));
    }
}
