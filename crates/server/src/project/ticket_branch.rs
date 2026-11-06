//! A ticket's worktree and branch, seen from the project it goes back into: review, merge,
//! discard.

use crate::worker::ticket::TicketId;
use std::path::{Path, PathBuf};

use composer_protocol::http::TicketReview;

use crate::project::git::{git, git_output, git_with};

/// Identity for the throwaway commit of uncommitted changes (never kept or shown).
const SCRATCH_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "composer"),
    ("GIT_AUTHOR_EMAIL", "composer@localhost"),
    ("GIT_COMMITTER_NAME", "composer"),
    ("GIT_COMMITTER_EMAIL", "composer@localhost"),
];

/// `state/worktrees/<ticket>` on `ticket/<ticket>`, and the checkout it was made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketBranch {
    pub ticket: String,
    pub worktree: PathBuf,
    /// The repository's main checkout: where the branch is merged.
    pub main: PathBuf,
    pub branch: String,
}

/// What the ticket branch would bring: its tip (with uncommitted changes as a scratch commit).
struct Tip {
    commit: String,
    uncommitted: bool,
}

impl TicketBranch {
    /// The ticket's worktree under `root` (`state/worktrees`); `None` when there is none.
    pub fn find(root: &Path, ticket: &TicketId) -> Result<Option<Self>, String> {
        let worktree = root.join(ticket.as_str());
        if !worktree.join(".git").exists() {
            return Ok(None);
        }
        // The first entry is the main checkout.
        let list = git(&worktree, &["worktree", "list", "--porcelain"])?;
        let main = list
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("worktree "))
            .ok_or_else(|| format!("{} has no main checkout", worktree.display()))?;
        let branch = git(&worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .map_err(|_| format!("{} is not on a branch", worktree.display()))?;
        Ok(Some(Self {
            ticket: ticket.to_string(),
            worktree,
            main: PathBuf::from(main),
            branch,
        }))
    }

    /// The branch checked out in the main checkout; `None` when its HEAD is detached.
    pub fn target(&self) -> Option<String> {
        git(&self.main, &["symbolic-ref", "--quiet", "--short", "HEAD"]).ok()
    }

    /// Where the ticket branch and the target branch (`into`, default: the main checkout's
    /// HEAD) part: what its changes are against.
    pub fn base(&self, into: Option<&str>) -> Result<String, String> {
        let head = match into {
            Some(b) => self.target_ref(b)?,
            None => git(&self.main, &["rev-parse", "HEAD"])?,
        };
        git(&self.worktree, &["merge-base", &head, "HEAD"])
    }

    /// Where merges land: `into` when it names an existing branch, else the checked-out one.
    fn merge_target(&self, into: Option<&str>) -> Result<Option<String>, String> {
        match into {
            Some(b) => {
                self.target_ref(b)?;
                Ok(Some(b.to_string()))
            }
            None => Ok(self.target()),
        }
    }

    /// Where `branch` is checked out (the main checkout included); `None` when nowhere.
    fn checkout_of(&self, branch: &str) -> Result<Option<PathBuf>, String> {
        let list = git(&self.main, &["worktree", "list", "--porcelain"])?;
        let wanted = format!("refs/heads/{branch}");
        for block in list.split("\n\n") {
            let mut path = None;
            let (mut on_branch, mut prunable) = (false, false);
            for line in block.lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(p));
                } else if line.strip_prefix("branch ") == Some(wanted.as_str()) {
                    on_branch = true;
                } else if line.starts_with("prunable") {
                    prunable = true;
                }
            }
            if on_branch {
                if prunable {
                    let dir = path.map(|p| p.display().to_string()).unwrap_or_default();
                    return Err(format!(
                        "{branch} is registered in the worktree {dir}, which no longer exists; run `git worktree prune` in the project and try again; nothing merged"
                    ));
                }
                return Ok(path);
            }
        }
        Ok(None)
    }

    /// The tip of `branch`, which must exist.
    fn target_ref(&self, branch: &str) -> Result<String, String> {
        git(
            &self.main,
            &[
                "rev-parse",
                "--quiet",
                "--verify",
                &format!("refs/heads/{branch}"),
            ],
        )
        .map_err(|_| format!("the project has no branch `{branch}`; nothing merged"))
    }

    /// Commits on the branch that the main checkout's HEAD doesn't have (uncommitted work not
    /// counted): cheap, for lists.
    pub fn commit_count(&self) -> Result<u32, String> {
        let head = git(&self.main, &["rev-parse", "HEAD"])?;
        Ok(git(
            &self.worktree,
            &["rev-list", "--count", &format!("{head}..HEAD")],
        )?
        .parse()
        .unwrap_or(0))
    }

    pub fn review(&self, into: Option<&str>) -> Result<TicketReview, String> {
        let tip = self.tip()?;
        let target = self.merge_target(into)?;
        let head = match &target {
            Some(b) => self.target_ref(b)?,
            None => git(&self.main, &["rev-parse", "HEAD"])?,
        };
        let ahead = git(
            &self.worktree,
            &["rev-list", "--count", &format!("{head}..{}", tip.commit)],
        )?
        .parse()
        .unwrap_or(0);
        let log = git(
            &self.worktree,
            &["log", "--format=%h %s", &format!("{head}..HEAD")],
        )?;
        Ok(TicketReview {
            ticket: self.ticket.clone(),
            branch: self.branch.clone(),
            target,
            ahead,
            commits: log.lines().map(str::to_string).collect(),
            uncommitted: tip.uncommitted,
            conflicts: self.conflicts(&head, &tip.commit)?,
        })
    }

    /// Merges the branch into `into` (default: the main checkout's checked-out branch),
    /// with `--no-ff`, then removes the worktree and the branch. A target checked out in a
    /// worktree (the main checkout or a linked one) is merged right there; one that isn't
    /// checked out anywhere is merged in a temporary worktree, leaving the main checkout as it is.
    /// Uncommitted changes in the worktree are committed first. Returns the merge commit.
    pub fn merge(&self, message: &str, into: Option<&str>) -> Result<String, String> {
        let Some(target) = self.merge_target(into)? else {
            return Err(format!(
                "{} has no branch checked out (detached HEAD); check one out first",
                self.main.display()
            ));
        };
        // git can only merge into what some worktree has in front of it.
        let checkout = self.checkout_of(&target)?;
        if let Some(dir) = &checkout
            && git(dir, &["rev-parse", "--quiet", "--verify", "MERGE_HEAD"]).is_ok()
        {
            return Err(format!(
                "{} is in the middle of a merge; finish or abort it first",
                dir.display()
            ));
        }
        let head = match into {
            Some(b) => self.target_ref(b)?,
            None => git(&self.main, &["rev-parse", "HEAD"])?,
        };
        let tip = self.tip()?;
        let conflicts = self.conflicts(&head, &tip.commit)?;
        if !conflicts.is_empty() {
            return Err(format!(
                "ticket {} conflicts with {target} in: {}. Ask its worker to merge {target} into \
                 its branch and resolve them, then merge again.",
                self.ticket,
                conflicts.join(", ")
            ));
        }
        if git(
            &self.worktree,
            &["rev-list", "--count", &format!("{head}..{}", tip.commit)],
        )? == "0"
        {
            return Err(format!(
                "ticket {} has nothing to merge: {} brings no changes",
                self.ticket, self.branch
            ));
        }
        if tip.uncommitted {
            git(&self.worktree, &["add", "-A"])?;
            git(
                &self.worktree,
                &[
                    "commit",
                    "-q",
                    "-m",
                    &format!("ticket {}: uncommitted changes", self.ticket),
                ],
            )
            .map_err(|e| format!("committing what the worker left uncommitted: {e}"))?;
        }
        // A target that isn't checked out gets a scratch worktree to merge in.
        let (repo, scratch) = if let Some(dir) = checkout.clone() {
            (dir, None)
        } else {
            let dir = std::env::temp_dir().join(format!(
                "composer-merge-{}-{}",
                self.ticket,
                std::process::id()
            ));
            let path = dir.to_string_lossy().to_string();
            git(&self.main, &["worktree", "add", &path, &target])
                .map_err(|e| format!("checking {target} out to merge into it: {e}"))?;
            (dir, Some(path))
        };
        let merged = git(
            &repo,
            &["merge", "--no-ff", "--no-edit", "-m", message, &self.branch],
        );
        if let Err(e) = merged {
            // Never leave anything half-merged.
            if git(&repo, &["rev-parse", "--quiet", "--verify", "MERGE_HEAD"]).is_ok() {
                let _ = git(&repo, &["merge", "--abort"]);
            }
            if let Some(path) = &scratch {
                let _ = git(&self.main, &["worktree", "remove", "--force", path]);
            }
            return Err(format!("merging {} into {target}: {e}", self.branch));
        }
        let commit = git(&repo, &["rev-parse", "--short", "HEAD"])?;
        if let Some(path) = &scratch {
            git(&self.main, &["worktree", "remove", "--force", path])
                .map_err(|e| format!("removing the scratch worktree {path}: {e}"))?;
        }
        // `git branch -d` measures against the checked-out HEAD, which isn't the target here.
        self.remove(checkout.as_deref() != Some(self.main.as_path()))?;
        Ok(commit)
    }

    /// Removes the worktree and deletes the branch without merging.
    pub fn discard(&self) -> Result<(), String> {
        self.remove(true)
    }

    fn remove(&self, force_branch: bool) -> Result<(), String> {
        let worktree = self.worktree.to_string_lossy();
        git(&self.main, &["worktree", "remove", "--force", &worktree])
            .map_err(|e| format!("removing the worktree {worktree}: {e}"))?;
        let delete = if force_branch { "-D" } else { "-d" };
        git(&self.main, &["branch", delete, &self.branch])
            .map_err(|e| format!("deleting {}: {e}", self.branch))?;
        Ok(())
    }

    /// The branch's tip, or a scratch commit on top of it holding the worktree's uncommitted
    /// changes (made through a temporary index: the worktree and its index stay as they are).
    fn tip(&self) -> Result<Tip, String> {
        let head = git(&self.worktree, &["rev-parse", "HEAD"])?;
        let git_dir = git(
            &self.worktree,
            &["rev-parse", "--path-format=absolute", "--git-dir"],
        )?;
        let index = Path::new(&git_dir).join("composer-review-index");
        let index_path = index.to_string_lossy().to_string();
        let mut env = vec![("GIT_INDEX_FILE", index_path.as_str())];
        let tree = git_with(&self.worktree, &["read-tree", "HEAD"], &env)
            .and_then(|_| git_with(&self.worktree, &["add", "-A"], &env))
            .and_then(|_| git_with(&self.worktree, &["write-tree"], &env));
        let _ = std::fs::remove_file(&index);
        let tree = tree?;
        if tree == git(&self.worktree, &["rev-parse", "HEAD^{tree}"])? {
            return Ok(Tip {
                commit: head,
                uncommitted: false,
            });
        }
        env.extend(SCRATCH_IDENTITY);
        let commit = git_with(
            &self.worktree,
            &[
                "commit-tree",
                &tree,
                "-p",
                &head,
                "-m",
                "uncommitted changes",
            ],
            &env,
        )?;
        Ok(Tip {
            commit,
            uncommitted: true,
        })
    }

    /// Files that would conflict merging `tip` into `head` (nothing is written but objects).
    fn conflicts(&self, head: &str, tip: &str) -> Result<Vec<String>, String> {
        let output = git_output(
            &self.main,
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                "--no-messages",
                head,
                tip,
            ],
            &[],
        )?;
        match output.status.code() {
            Some(0) => Ok(vec![]),
            // First the tree, then one conflicted file per line.
            Some(1) => Ok(String::from_utf8_lossy(&output.stdout)
                .lines()
                .skip(1)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()),
            _ => Err(format!(
                "checking ticket {} against {}: {}",
                self.ticket,
                self.main.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ticket_worktree;

    fn repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["add", "."],
            &["commit", "-qm", "init"],
        ] {
            git(dir, args).unwrap();
        }
    }

    fn commit(dir: &Path, file: &str, text: &str) {
        std::fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", "-A"]).unwrap();
        git(dir, &["commit", "-qm", file]).unwrap();
    }

    fn setup(ticket: &str) -> (tempfile::TempDir, PathBuf, TicketBranch) {
        let tmp = tempfile::tempdir().unwrap();
        let (project, root) = (tmp.path().join("p"), tmp.path().join("wt"));
        repo(&project);
        let ticket = TicketId::parse(ticket).unwrap();
        ticket_worktree(&project, &root, &ticket).unwrap().unwrap();
        let branch = TicketBranch::find(&root, &ticket).unwrap().unwrap();
        (tmp, project, branch)
    }

    #[test]
    fn reviews_then_merges_committed_and_uncommitted_work() {
        let (_tmp, project, branch) = setup("0001");
        assert_eq!(branch.branch, "ticket/0001");
        assert_eq!(
            branch.main.canonicalize().unwrap(),
            project.canonicalize().unwrap()
        );

        commit(&branch.worktree, "b.txt", "new\n");
        std::fs::write(branch.worktree.join("c.txt"), "left over\n").unwrap();
        // The project moves on meanwhile, elsewhere.
        commit(&project, "d.txt", "main\n");

        let review = branch.review(None).unwrap();
        assert_eq!(review.target.as_deref(), Some("main"));
        assert_eq!(review.ahead, 2, "the commit and the uncommitted changes");
        assert_eq!(review.commits.len(), 1);
        assert!(review.commits[0].ends_with(" b.txt"));
        assert!(review.uncommitted);
        assert!(review.conflicts.is_empty());
        // Reviewing changed nothing in the worktree.
        assert!(
            git(&branch.worktree, &["status", "--porcelain"])
                .unwrap()
                .contains("?? c.txt")
        );

        let commit = branch.merge("Merge ticket 0001: B", None).unwrap();
        assert_eq!(
            git(&project, &["log", "-1", "--format=%h %s"]).unwrap(),
            format!("{commit} Merge ticket 0001: B")
        );
        assert!(project.join("b.txt").is_file() && project.join("c.txt").is_file());
        assert!(!branch.worktree.exists());
        assert!(
            git(
                &project,
                &["rev-parse", "--verify", "--quiet", "ticket/0001"]
            )
            .is_err()
        );
    }

    #[test]
    fn conflicts_are_refused_and_change_nothing() {
        let (_tmp, project, branch) = setup("0002");
        commit(&branch.worktree, "a.txt", "one\nTWO\nthree\n");
        commit(&project, "a.txt", "one\n2\nthree\n");

        assert_eq!(branch.review(None).unwrap().conflicts, ["a.txt"]);
        let error = branch.merge("m", None).unwrap_err();
        assert!(error.contains("conflicts with main in: a.txt"), "{error}");
        assert_eq!(
            git(&project, &["log", "-1", "--format=%s"]).unwrap(),
            "a.txt"
        );
        assert!(
            git(&project, &["status", "--porcelain"])
                .unwrap()
                .is_empty()
        );
        assert!(branch.worktree.exists());
    }

    #[test]
    fn local_changes_in_the_project_stop_the_merge_safely() {
        let (_tmp, project, branch) = setup("0003");
        commit(&branch.worktree, "a.txt", "one\nTWO\nthree\n");
        std::fs::write(project.join("a.txt"), "edited here\n").unwrap();
        let main_tip = git(&project, &["rev-parse", "main"]).unwrap();

        let error = branch.merge("m", None).unwrap_err();
        assert!(error.contains("merging ticket/0003 into main"), "{error}");
        assert_eq!(
            std::fs::read_to_string(project.join("a.txt")).unwrap(),
            "edited here\n"
        );
        assert!(
            git(
                &project,
                &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]
            )
            .is_err()
        );
        assert_eq!(git(&project, &["rev-parse", "main"]).unwrap(), main_tip);
        assert!(branch.worktree.exists());
    }

    #[test]
    fn a_target_in_a_vanished_worktree_asks_for_a_prune() {
        let (tmp, project, branch) = setup("0008");
        let gone = tmp.path().join("gone");
        git(
            &project,
            &["worktree", "add", "-b", "dev", gone.to_str().unwrap()],
        )
        .unwrap();
        std::fs::remove_dir_all(&gone).unwrap();
        commit(&branch.worktree, "b.txt", "new\n");
        let dev_tip = git(&project, &["rev-parse", "dev"]).unwrap();

        let error = branch.merge("m", Some("dev")).unwrap_err();
        assert!(error.contains("git worktree prune"), "{error}");
        assert!(!error.contains("fatal"), "{error}");
        assert_eq!(git(&project, &["rev-parse", "dev"]).unwrap(), dev_tip);
        assert!(branch.worktree.exists());
    }

    #[test]
    fn merges_into_another_branch_in_a_scratch_worktree() {
        let (_tmp, project, branch) = setup("0005");
        git(&project, &["branch", "dev", "main"]).unwrap();
        commit(&branch.worktree, "b.txt", "new\n");
        std::fs::write(project.join("dirty.txt"), "untouched\n").unwrap();

        let review = branch.review(Some("dev")).unwrap();
        assert_eq!(review.target.as_deref(), Some("dev"));
        assert_eq!(review.ahead, 1);

        let commit = branch.merge("m", Some("dev")).unwrap();
        assert_eq!(
            git(&project, &["log", "-1", "--format=%h %s", "dev"]).unwrap(),
            format!("{commit} m")
        );
        assert_eq!(
            git(&project, &["log", "-1", "--format=%s"]).unwrap(),
            "init"
        );
        assert_eq!(
            std::fs::read_to_string(project.join("dirty.txt")).unwrap(),
            "untouched\n"
        );
        assert!(
            !project.join("b.txt").exists(),
            "the main checkout stays as it was"
        );
        assert!(!branch.worktree.exists());
        assert!(
            git(
                &project,
                &["rev-parse", "--verify", "--quiet", "ticket/0005"]
            )
            .is_err()
        );
    }

    #[test]
    fn merges_into_a_branch_checked_out_in_another_worktree() {
        let (tmp, project, branch) = setup("0007");
        let other = tmp.path().join("other");
        git(
            &project,
            &["worktree", "add", "-b", "dev", other.to_str().unwrap()],
        )
        .unwrap();
        commit(&branch.worktree, "b.txt", "new\n");

        let commit = branch.merge("m", Some("dev")).unwrap();
        assert_eq!(
            git(&other, &["log", "-1", "--format=%h %s"]).unwrap(),
            format!("{commit} m")
        );
        assert!(
            other.join("b.txt").is_file(),
            "the checkout reflects the merge"
        );
        assert!(git(&other, &["status", "--porcelain"]).unwrap().is_empty());
        assert!(!project.join("b.txt").exists());
        assert!(!branch.worktree.exists());
        assert!(
            !git(&project, &["worktree", "list"])
                .unwrap()
                .contains("composer-merge")
        );
    }

    #[test]
    fn a_merge_in_progress_in_the_target_worktree_stops_the_merge() {
        let (tmp, project, branch) = setup("0008");
        let other = tmp.path().join("other");
        git(
            &project,
            &["worktree", "add", "-b", "dev", other.to_str().unwrap()],
        )
        .unwrap();
        commit(&branch.worktree, "b.txt", "new\n");
        let git_dir = git(
            &other,
            &["rev-parse", "--path-format=absolute", "--git-dir"],
        )
        .unwrap();
        let head = git(&other, &["rev-parse", "HEAD"]).unwrap();
        std::fs::write(Path::new(&git_dir).join("MERGE_HEAD"), format!("{head}\n")).unwrap();

        let error = branch.merge("m", Some("dev")).unwrap_err();
        assert!(error.contains("in the middle of a merge"), "{error}");
        assert!(branch.worktree.exists());
    }

    #[test]
    fn naming_the_checked_out_branch_merges_in_the_main_checkout() {
        let (_tmp, project, branch) = setup("0009");
        commit(&branch.worktree, "b.txt", "new\n");
        branch.merge("m", Some("main")).unwrap();
        assert!(project.join("b.txt").is_file());
        assert!(!branch.worktree.exists());
    }

    #[test]
    fn an_unknown_target_branch_errors_cleanly() {
        let (_tmp, project, branch) = setup("0006");
        commit(&branch.worktree, "b.txt", "new\n");
        let error = branch.merge("m", Some("ghost")).unwrap_err();
        assert!(error.contains("no branch `ghost`"), "{error}");
        assert!(branch.worktree.exists());
        assert_eq!(
            git(&project, &["log", "-1", "--format=%s"]).unwrap(),
            "init"
        );
    }

    #[test]
    fn nothing_to_merge_and_discard() {
        let (_tmp, project, branch) = setup("0004");
        assert!(
            branch
                .merge("m", None)
                .unwrap_err()
                .contains("nothing to merge")
        );

        commit(&branch.worktree, "b.txt", "x\n");
        branch.discard().unwrap();
        assert!(!branch.worktree.exists());
        assert!(
            git(
                &project,
                &["rev-parse", "--verify", "--quiet", "ticket/0004"]
            )
            .is_err()
        );
        assert!(!project.join("b.txt").exists());
    }
}
