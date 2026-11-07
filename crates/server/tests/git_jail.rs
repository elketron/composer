//! Composer's own git runs in a jail (ticket 0161): what a sandboxed run plants in a repository
//! it can write (a `post-merge` hook, `core.fsmonitor`, a filter driver) still runs when composer
//! merges or diffs its ticket, but only inside: it can't write outside the repository, and the
//! merge goes through. A worktree whose `.git` file was rewritten to name another repository is
//! refused, and the semantic diff never reads through a symlink a run committed.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

use std::path::{Path, PathBuf};

use composer_server::project::TicketBranch;
use composer_server::project::semantic_diff;
use composer_server::sandbox::GitRepo;
use composer_server::worker::ticket::TicketId;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A script, run once as a probe until it starts: another test's fork may still hold it open for
/// writing ("text file busy"), and git would then skip a hook.
fn executable(path: &Path, text: &str) {
    let (shebang, body) = text.split_once('\n').unwrap();
    std::fs::write(
        path,
        format!("{shebang}\n[ -n \"$PROBE\" ] && exit 0\n{body}"),
    )
    .unwrap();
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    for _ in 0..100 {
        match std::process::Command::new(path).env("PROBE", "1").status() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => break,
        }
    }
}

struct Repo {
    _dir: tempfile::TempDir,
    root: PathBuf,
    project: PathBuf,
    worktrees: PathBuf,
}

/// A project with ticket 0001's worktree (one commit on `ticket/0001`).
fn repo() -> Repo {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let (project, worktrees) = (root.join("project"), root.join("worktrees"));
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("a.txt"), "a\n").unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["add", "."]);
    git(&project, &["commit", "-qm", "init"]);
    let worktree = worktrees.join("0001");
    git(
        &project,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "ticket/0001",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("b.txt"), "b\n").unwrap();
    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "work"]);
    Repo {
        _dir: dir,
        root,
        project,
        worktrees,
    }
}

#[test]
fn what_a_run_plants_in_the_repo_runs_only_inside_it() {
    let r = repo();
    let outside = r.root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let git_dir = r.project.join(".git");
    // What a sandboxed run with the repository writable could leave: one script, called as a
    // hook, as fsmonitor and as a filter; it marks the repository (allowed) and outside (not).
    let evil = git_dir.join("evil.sh");
    executable(
        &evil,
        &format!(
            "#!/bin/sh\ntouch '{git}/ran-'\"$1\"\ntouch '{out}/escaped-'\"$1\" 2>/dev/null\n\
             case \"$PWD\" in '{worktrees}'*) touch '{main}/escaped-'\"$1\"'-from-the-worktree' \
             2>/dev/null;; esac\n[ \"$1\" = filter ] && cat\nexit 0\n",
            git = git_dir.display(),
            out = outside.display(),
            worktrees = r.worktrees.display(),
            main = r.project.display(),
        ),
    );
    executable(
        &git_dir.join("hooks/post-merge"),
        &format!("#!/bin/sh\nexec '{}' hook\n", evil.display()),
    );
    git(
        &r.project,
        &[
            "config",
            "core.fsmonitor",
            &format!("{} fsmonitor", evil.display()),
        ],
    );
    git(
        &r.project,
        &[
            "config",
            "filter.evil.clean",
            &format!("{} filter", evil.display()),
        ],
    );
    std::fs::write(git_dir.join("info/attributes"), "*.txt filter=evil\n").unwrap();

    // Uncommitted work: the review stages it in the worktree, where the filter runs with only the
    // worktree and the main `.git`, not the main checkout.
    std::fs::write(r.worktrees.join("0001/c.txt"), "c\n").unwrap();
    let branch = TicketBranch::find(&r.worktrees, &TicketId::parse("0001").unwrap())
        .unwrap()
        .unwrap();
    branch.review(None).unwrap();
    assert!(git_dir.join("ran-filter").exists(), "the filter still runs");
    let from_worktree: Vec<_> = std::fs::read_dir(&r.project)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with("from-the-worktree"))
        .collect();
    assert!(
        from_worktree.is_empty(),
        "reached the main checkout: {from_worktree:?}"
    );
    branch.merge("Merge ticket 0001", None).unwrap();

    assert!(r.project.join("b.txt").is_file(), "the merge went through");
    assert!(git_dir.join("ran-hook").exists(), "the hook still runs");
    let escaped: Vec<_> = std::fs::read_dir(&outside)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert!(escaped.is_empty(), "escaped the jail: {escaped:?}");
}

#[test]
fn a_worktree_naming_another_repository_is_refused() {
    let r = repo();
    let worktree = r.worktrees.join("0001");
    // Another repository, with a worktree of its own: the run points its `.git` file there.
    let other = repo();
    let theirs = other.project.join(".git/worktrees/0001");
    assert!(theirs.is_dir());
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", theirs.display()),
    )
    .unwrap();

    let refused = GitRepo::discover(&worktree).unwrap_err();
    assert!(refused.contains("names it back"), "{refused}");
    let error = TicketBranch::find(&r.worktrees, &TicketId::parse("0001").unwrap()).unwrap_err();
    assert!(error.contains("names it back"), "{error}");
    // Its own worktree, untouched, is fine.
    assert!(matches!(
        GitRepo::discover(&other.worktrees.join("0001")),
        Ok(Some(GitRepo::Worktree { .. }))
    ));
}

#[tokio::test]
async fn the_semantic_diff_never_reads_through_a_symlink() {
    let r = repo();
    let worktree = r.worktrees.join("0001");
    let secret = r.root.join("id_ed25519");
    std::fs::write(&secret, "SECRET KEY\n").unwrap();
    std::fs::remove_file(worktree.join("a.txt")).unwrap();
    std::os::unix::fs::symlink(&secret, worktree.join("a.txt")).unwrap();
    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-qm", "link"]);
    // A difft that prints what it was given, as if it were a structural diff.
    let difft = r.root.join("difft");
    executable(
        &difft,
        "#!/bin/sh\nfor last; do :; done\necho 'a.txt --- Rust'\ncat \"$last\"\n",
    );

    let shown = semantic_diff(&difft, &worktree, "main", "a.txt", "M").await;
    assert!(
        !shown.unwrap_or_default().contains("SECRET"),
        "read through the symlink"
    );
}
