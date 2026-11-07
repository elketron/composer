//! `project_diff` looks difft up once, and without it does none of the per-file difft work
//! (no `git show` of the old content). Watched through a `git` wrapper on a temp PATH.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use composer_server::project::project_diff;

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

#[tokio::test]
async fn no_difft_means_no_git_show_and_difft_means_one_per_file() {
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join("git"))
        .find(|p| p.is_file())
        .expect("git");
    let project = tempfile::tempdir().unwrap();
    let dir = project.path();
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(dir.join("b.rs"), "fn b() {}\n").unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["add", "."],
        &["commit", "-qm", "init"],
    ] {
        git(dir, args);
    }
    std::fs::write(dir.join("a.rs"), "fn a() { 1 }\n").unwrap();
    std::fs::write(dir.join("b.rs"), "fn b() { 2 }\n").unwrap();

    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("git.log");
    script(
        &bin.path().join("git"),
        &format!(
            "echo \"$@\" >> {}\nexec {} \"$@\"",
            log.display(),
            real_git.display()
        ),
    );
    let shows = || {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains(" show "))
            .count()
    };
    // This counts calls to a git on PATH; the git jail would run the real one inside.
    let mut off = composer_server::Config::defaults().sandbox;
    off.enabled = false;
    composer_server::sandbox::configure_git_jail(&off);
    let old_path = std::env::var_os("PATH").unwrap();
    unsafe { std::env::set_var("PATH", bin.path()) };

    let plain = project_diff(dir, None).await.unwrap();
    assert!(plain.files.iter().all(|f| f.semantic.is_none()));
    assert_eq!(plain.files.len(), 2);
    assert_eq!(shows(), 0, "no difft: the old content is never fetched");

    script(&bin.path().join("difft"), "printf 'x --- Rust\\n1 2 y\\n'");
    let rich = project_diff(dir, None).await.unwrap();
    unsafe { std::env::set_var("PATH", old_path) };
    assert!(rich.files.iter().all(|f| f.semantic.is_some()));
    assert_eq!(shows(), 2, "difft present: one old-content fetch per file");
}
