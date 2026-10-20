//! Per-ticket git worktrees, so workers on the same project don't collide.

use std::path::{Path, PathBuf};

use crate::project::git::git;

/// Where a ticket's worker works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// The folder to work in: the worktree (or the matching subfolder, when the project is one).
    pub path: PathBuf,
    /// `ticket/<id>`.
    pub branch: String,
    /// The commit it started from (`None` when an existing worktree was reused).
    pub base: Option<String>,
}

/// The worktree for `ticket` of `project`, under `root` (`state/worktrees`): reused if it is
/// already there, else made on branch `ticket/<id>` (new, from the project's HEAD, or the
/// existing branch). `None` when the project isn't a git checkout with a commit; the worker
/// then works in the project folder itself.
pub fn ticket_worktree(
    project: &Path,
    root: &Path,
    ticket: &str,
) -> Result<Option<Worktree>, String> {
    let Ok(top) = git(project, &["rev-parse", "--show-toplevel"]) else {
        return Ok(None);
    };
    let Ok(head) = git(project, &["rev-parse", "--verify", "--quiet", "HEAD"]) else {
        return Ok(None);
    };
    let top = PathBuf::from(top);
    let inside = project
        .canonicalize()
        .ok()
        .and_then(|p| {
            p.strip_prefix(top.canonicalize().ok()?)
                .ok()
                .map(Path::to_path_buf)
        })
        .unwrap_or_default();
    let branch = format!("ticket/{ticket}");
    let dir = root.join(ticket);
    // (`join("")` would add a trailing slash.)
    let folder = |dir: &Path| {
        if inside.as_os_str().is_empty() {
            dir.to_path_buf()
        } else {
            dir.join(&inside)
        }
    };

    if dir.exists() {
        let mine = git(&dir, &["rev-parse", "--show-toplevel"])
            .ok()
            .and_then(|t| PathBuf::from(t).canonicalize().ok())
            == dir.canonicalize().ok();
        let same_repo = git(&dir, &["rev-parse", "--git-common-dir"])
            .ok()
            .map(|d| dir.join(d))
            .and_then(|d| d.canonicalize().ok())
            == git(&top, &["rev-parse", "--git-common-dir"])
                .ok()
                .map(|d| top.join(d))
                .and_then(|d| d.canonicalize().ok());
        if mine && same_repo {
            return Ok(Some(Worktree {
                path: folder(&dir),
                branch,
                base: None,
            }));
        }
        return Err(format!(
            "{} exists but is not a worktree of {}; remove it or rename the ticket",
            dir.display(),
            top.display()
        ));
    }

    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let target = dir.to_string_lossy().to_string();
    let exists = git(
        &top,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok();
    let (args, base): (Vec<&str>, Option<String>) = if exists {
        (vec!["worktree", "add", &target, &branch], None)
    } else {
        (
            vec!["worktree", "add", "-b", &branch, &target, &head],
            Some(head.clone()),
        )
    };
    git(&top, &args).map_err(|e| format!("creating the worktree for ticket {ticket}: {e}"))?;
    Ok(Some(Worktree {
        path: folder(&dir),
        branch,
        base,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(dir: &Path) {
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::write(dir.join("app/main.rs"), "fn main() {}\n").unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["add", "."],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "init",
            ],
        ] {
            git(dir, args).unwrap();
        }
    }

    #[test]
    fn makes_then_reuses_a_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let (project, root) = (tmp.path().join("p"), tmp.path().join("state/worktrees"));
        repo(&project);

        let made = ticket_worktree(&project, &root, "0003").unwrap().unwrap();
        assert_eq!(made.branch, "ticket/0003");
        assert_eq!(made.path, root.join("0003"));
        assert!(made.base.is_some());
        assert!(made.path.join("app/main.rs").is_file());

        // Again (a retried ticket): the same worktree, kept as it is.
        std::fs::write(made.path.join("wip.txt"), "x").unwrap();
        let again = ticket_worktree(&project, &root, "0003").unwrap().unwrap();
        assert_eq!((again.path.clone(), again.base), (made.path.clone(), None));
        assert!(again.path.join("wip.txt").is_file());

        // Removed, but the branch is kept: checked out again.
        git(
            &project,
            &["worktree", "remove", "--force", made.path.to_str().unwrap()],
        )
        .unwrap();
        let back = ticket_worktree(&project, &root, "0003").unwrap().unwrap();
        assert_eq!(back.base, None);
        assert_eq!(
            git(&back.path, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap(),
            "ticket/0003"
        );
    }

    #[test]
    fn a_subfolder_project_maps_into_the_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let (project, root) = (tmp.path().join("p"), tmp.path().join("wt"));
        repo(&project);
        let made = ticket_worktree(&project.join("app"), &root, "0004")
            .unwrap()
            .unwrap();
        assert_eq!(made.path, root.join("0004/app"));
        assert!(made.path.join("main.rs").is_file());
    }

    #[test]
    fn plain_folders_and_empty_repos_have_none() {
        let tmp = tempfile::tempdir().unwrap();
        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(
            ticket_worktree(&plain, &tmp.path().join("wt"), "1").unwrap(),
            None
        );
        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        git(&empty, &["init", "-q"]).unwrap();
        assert_eq!(
            ticket_worktree(&empty, &tmp.path().join("wt"), "1").unwrap(),
            None
        );
    }

    #[test]
    fn a_stranger_folder_in_the_way_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (project, root) = (tmp.path().join("p"), tmp.path().join("wt"));
        repo(&project);
        std::fs::create_dir_all(root.join("0005")).unwrap();
        assert!(
            ticket_worktree(&project, &root, "0005")
                .unwrap_err()
                .contains("not a worktree")
        );
    }
}
