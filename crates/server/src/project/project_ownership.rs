use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::project::ProjectPath;
use crate::workspace_name::WorkspaceName;

/// Which workspace owns which project folder: one index for the whole host (`state/projects.json`
/// at the home root), so a project belongs to exactly one workspace. Nested folders count as the
/// same project: a workspace can't take `~/x/sub` while another owns `~/x`, or the other way.
#[derive(Debug)]
pub struct ProjectOwnership {
    file: Option<PathBuf>,
    owners: Mutex<BTreeMap<ProjectPath, WorkspaceName>>,
}

impl ProjectOwnership {
    /// The index saved at `file` (empty when missing or unreadable; unknown paths are dropped).
    pub fn load(file: PathBuf) -> Self {
        let saved: BTreeMap<String, String> = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let owners = saved
            .into_iter()
            .filter_map(|(path, owner)| {
                let path = ProjectPath::existing(std::path::Path::new(&path)).ok()?;
                Some((path, WorkspaceName::new(&owner).ok()?))
            })
            .collect();
        Self {
            file: Some(file),
            owners: Mutex::new(owners),
        }
    }

    /// An index kept in memory only (a server serving one workspace alone).
    pub fn in_memory() -> Self {
        Self {
            file: None,
            owners: Mutex::new(BTreeMap::new()),
        }
    }

    /// The workspace owning `project`, or a folder it is nested in or contains, if any.
    pub fn owner(&self, project: &ProjectPath) -> Option<(ProjectPath, WorkspaceName)> {
        let owners = self.owners.lock().expect("ownership lock");
        owners
            .iter()
            .find(|(path, _)| path.overlaps(project))
            .map(|(path, owner)| (path.clone(), owner.clone()))
    }

    /// Whether `path` (canonical; a folder or a file) lies in, or contains, a project another
    /// workspace owns: what a mount inside `workspace`'s web may not point at.
    pub fn refuses(&self, path: &std::path::Path, workspace: &WorkspaceName) -> bool {
        let owners = self.owners.lock().expect("ownership lock");
        owners.iter().any(|(owned, owner)| {
            owner != workspace
                && (path.starts_with(owned.as_path()) || owned.as_path().starts_with(path))
        })
    }

    /// Fails when `project` (or a folder overlapping it) belongs to another workspace.
    pub fn check(&self, project: &ProjectPath, workspace: &WorkspaceName) -> Result<(), String> {
        match self.owner(project) {
            Some((owned, owner)) if &owner != workspace => Err(refusal(project, &owned, &owner)),
            _ => Ok(()),
        }
    }

    /// Records `project` as `workspace`'s, unless it overlaps another workspace's project. A
    /// project already owned (or inside a folder already owned) by `workspace` stays as it is.
    pub fn claim(&self, project: &ProjectPath, workspace: &WorkspaceName) -> Result<(), String> {
        let mut owners = self.owners.lock().expect("ownership lock");
        if let Some((owned, owner)) = owners.iter().find(|(path, _)| path.overlaps(project)) {
            if owner != workspace {
                return Err(refusal(project, owned, owner));
            }
            if project.as_path().starts_with(owned.as_path()) {
                return Ok(());
            }
        }
        owners.insert(project.clone(), workspace.clone());
        self.save(&owners);
        Ok(())
    }

    fn save(&self, owners: &BTreeMap<ProjectPath, WorkspaceName>) {
        let Some(file) = &self.file else {
            return;
        };
        let saved: BTreeMap<String, String> = owners
            .iter()
            .map(|(path, owner)| (path.to_string(), owner.to_string()))
            .collect();
        let text = serde_json::to_string_pretty(&saved).expect("ownership serializes");
        if let Err(e) = crate::init::write_creating_dirs(file, text.as_bytes()) {
            tracing::warn!("saving project ownership to {}: {e}", file.display());
        }
    }
}

/// What an agent (or user) reads when a project is another workspace's.
fn refusal(project: &ProjectPath, owned: &ProjectPath, owner: &WorkspaceName) -> String {
    let via = if owned == project {
        String::new()
    } else {
        format!(" (through {owned})")
    };
    format!(
        "{project} belongs to workspace `{owner}`{via}: a project belongs to one workspace only. \
         Work on it from `{owner}`, or pick a project of this workspace."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, impl Fn(&str) -> ProjectPath) {
        let dir = tempfile::tempdir().unwrap();
        for d in ["x/sub", "y"] {
            std::fs::create_dir_all(dir.path().join(d)).unwrap();
        }
        let root = dir.path().to_path_buf();
        (dir, move |d: &str| {
            ProjectPath::existing(&root.join(d)).unwrap()
        })
    }

    #[test]
    fn a_project_belongs_to_one_workspace_nesting_included() {
        let (_dir, p) = setup();
        let (a, b) = (
            WorkspaceName::new("a").unwrap(),
            WorkspaceName::new("b").unwrap(),
        );
        let index = ProjectOwnership::in_memory();
        index.claim(&p("x"), &a).unwrap();
        assert!(
            index
                .claim(&p("x"), &b)
                .unwrap_err()
                .contains("workspace `a`")
        );
        assert!(index.claim(&p("x/sub"), &b).is_err(), "inside a's project");
        assert!(index.check(&p("x/sub"), &b).is_err());
        index.claim(&p("x/sub"), &a).unwrap();
        index.claim(&p("y"), &b).unwrap();
        assert!(index.check(&p("y"), &a).is_err());
        index.check(&p("y"), &b).unwrap();
    }

    #[test]
    fn a_folder_containing_another_workspaces_project_is_refused() {
        let (_dir, p) = setup();
        let (a, b) = (
            WorkspaceName::new("a").unwrap(),
            WorkspaceName::new("b").unwrap(),
        );
        let index = ProjectOwnership::in_memory();
        index.claim(&p("x/sub"), &a).unwrap();
        let error = index.claim(&p("x"), &b).unwrap_err();
        assert!(error.contains("through"), "{error}");
    }

    #[test]
    fn survives_a_restart() {
        let (dir, p) = setup();
        let file = dir.path().join("state/projects.json");
        let a = WorkspaceName::new("a").unwrap();
        ProjectOwnership::load(file.clone())
            .claim(&p("x"), &a)
            .unwrap();
        let again = ProjectOwnership::load(file);
        assert_eq!(again.owner(&p("x/sub")).unwrap().1, a);
    }
}
