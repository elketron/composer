//! A ticket's worktree and branch, seen from the project it goes back into: review, merge,
//! discard.

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
    pub fn find(root: &Path, ticket: &str) -> Result<Option<Self>, String> {
        let worktree = root.join(ticket);
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

    /// Where the ticket branch and the main checkout's HEAD part: what its changes are against.
    pub fn base(&self) -> Result<String, String> {
        let head = git(&self.main, &["rev-parse", "HEAD"])?;
        git(&self.worktree, &["merge-base", &head, "HEAD"])
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

    pub fn review(&self) -> Result<TicketReview, String> {
        let tip = self.tip()?;
        let head = git(&self.main, &["rev-parse", "HEAD"])?;
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
            target: self.target(),
            ahead,
            commits: log.lines().map(str::to_string).collect(),
            uncommitted: tip.uncommitted,
            conflicts: self.conflicts(&head, &tip.commit)?,
        })
    }

    /// Merges the branch into the main checkout (`--no-ff`), then removes the worktree and the
    /// branch. Uncommitted changes in the worktree are committed first. Returns the merge commit.
    pub fn merge(&self, message: &str) -> Result<String, String> {
        let Some(target) = self.target() else {
            return Err(format!(
                "{} has no branch checked out (detached HEAD); check one out first",
                self.main.display()
            ));
        };
        if git(
            &self.main,
            &["rev-parse", "--quiet", "--verify", "MERGE_HEAD"],
        )
        .is_ok()
        {
            return Err(format!(
                "{} is in the middle of a merge; finish or abort it first",
                self.main.display()
            ));
        }
        let head = git(&self.main, &["rev-parse", "HEAD"])?;
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
        let merged = git(
            &self.main,
            &["merge", "--no-ff", "--no-edit", "-m", message, &self.branch],
        );
        if let Err(e) = merged {
            // Never leave the project half-merged.
            if git(
                &self.main,
                &["rev-parse", "--quiet", "--verify", "MERGE_HEAD"],
            )
            .is_ok()
            {
                let _ = git(&self.main, &["merge", "--abort"]);
            }
            return Err(format!("merging {} into {target}: {e}", self.branch));
        }
        let commit = git(&self.main, &["rev-parse", "--short", "HEAD"])?;
        self.remove(false)?;
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
        ticket_worktree(&project, &root, ticket).unwrap().unwrap();
        let branch = TicketBranch::find(&root, ticket).unwrap().unwrap();
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

        let review = branch.review().unwrap();
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

        let commit = branch.merge("Merge ticket 0001: B").unwrap();
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

        assert_eq!(branch.review().unwrap().conflicts, ["a.txt"]);
        let error = branch.merge("m").unwrap_err();
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

        let error = branch.merge("m").unwrap_err();
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
        assert!(branch.worktree.exists());
    }

    #[test]
    fn nothing_to_merge_and_discard() {
        let (_tmp, project, branch) = setup("0004");
        assert!(branch.merge("m").unwrap_err().contains("nothing to merge"));

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
