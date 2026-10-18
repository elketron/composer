//! Working-tree changes of a project, from git.

use std::path::Path;

use composer_protocol::http::{ChangedFile, ProjectDiff};
use tokio::process::Command;

/// git's empty tree: what a repository without commits is compared against.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// New (untracked) files are shown in full up to this size, and this many of them.
const UNTRACKED_MAX_BYTES: u64 = 64 * 1024;
const UNTRACKED_MAX_FILES: usize = 30;
/// The diff is cut beyond this, so a huge change doesn't flood the client.
const DIFF_MAX_BYTES: usize = 512 * 1024;

/// Changes of the working tree (staged or not) against HEAD, plus untracked files.
pub async fn project_diff(path: &Path) -> Result<ProjectDiff, String> {
    if !path.join(".git").exists() {
        let inside = git(path, &["rev-parse", "--is-inside-work-tree"]).await;
        if inside.as_deref().map(str::trim) != Ok("true") {
            return Err(format!("{} is not a git checkout", path.display()));
        }
    }
    let branch = git(path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await
        .ok()
        .map(|b| b.trim().to_string());
    let base = if git(path, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .await
        .is_ok()
    {
        "HEAD"
    } else {
        EMPTY_TREE
    };

    let mut files = Vec::new();
    let numstat = git(path, &["diff", base, "--numstat", "-z", "--no-renames"]).await?;
    let status = git(path, &["diff", base, "--name-status", "-z", "--no-renames"]).await?;
    let statuses: Vec<&str> = status.split('\0').filter(|s| !s.is_empty()).collect();
    for entry in numstat.split('\0').filter(|s| !s.is_empty()) {
        let mut parts = entry.splitn(3, '\t');
        let (added, removed, file) = (parts.next(), parts.next(), parts.next().unwrap_or(""));
        // --name-status -z gives "M\0path\0"; find this file's letter.
        let letter = statuses
            .windows(2)
            .find(|w| w[1] == file)
            .map(|w| w[0])
            .unwrap_or("M");
        files.push(ChangedFile {
            path: file.to_string(),
            status: letter.chars().next().unwrap_or('M').to_string(),
            added: added.and_then(|a| a.parse().ok()).unwrap_or(0),
            removed: removed.and_then(|r| r.parse().ok()).unwrap_or(0),
        });
    }
    let mut diff = git(
        path,
        &[
            "diff",
            base,
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            "-U3",
        ],
    )
    .await?;

    let untracked = git(path, &["ls-files", "--others", "--exclude-standard", "-z"]).await?;
    for file in untracked
        .split('\0')
        .filter(|s| !s.is_empty())
        .take(UNTRACKED_MAX_FILES)
    {
        let full = path.join(file);
        let size = std::fs::metadata(&full)
            .map(|m| m.len())
            .unwrap_or(u64::MAX);
        let text = if size <= UNTRACKED_MAX_BYTES {
            std::fs::read_to_string(&full).ok()
        } else {
            None
        };
        let added = text.as_ref().map_or(0, |t| t.lines().count() as u32);
        files.push(ChangedFile {
            path: file.to_string(),
            status: "?".into(),
            added,
            removed: 0,
        });
        if let Some(text) = text {
            diff.push_str(&new_file_diff(file, &text));
        }
    }

    if diff.len() > DIFF_MAX_BYTES {
        let cut = diff.floor_char_boundary(DIFF_MAX_BYTES);
        diff.truncate(cut);
        diff.push_str("\n… (diff cut; it is very large)\n");
    }
    Ok(ProjectDiff {
        path: path.display().to_string(),
        branch,
        files,
        diff,
    })
}

/// A unified diff that adds `text` as a new file.
fn new_file_diff(file: &str, text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = format!(
        "diff --git a/{file} b/{file}\nnew file\n--- /dev/null\n+++ b/{file}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for line in lines {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

async fn git(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .await
        .map_err(|e| format!("could not run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
