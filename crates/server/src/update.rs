//! `composer update`: brings the stock sites and templates up to this binary's versions without
//! losing the user's (or the agents') changes: the home's library and templates, and each
//! workspace's own copy of the stock sites.
//!
//! Every stock file installed is also kept in `state/seed/` (the home's or the workspace's) as
//! the base. Per file: unchanged since installed → replaced; changed by the user and upstream →
//! three-way merge (`git merge-file`); a conflict, or no base to merge from, leaves the user's
//! file alone and puts the new version in `state/updates/` to look at. Either way the new version
//! becomes the base, so the next update doesn't report the file again: `state/updates/` is the
//! to-do list.

use std::path::{Path, PathBuf};
use std::process::Command;

use include_dir::Dir;

use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::init::{approve_seed_recipes, stock_message, stock_site_names, write_creating_dirs};
use crate::library::{Library, SiteName, WorkspaceLibrary};
use crate::stock_root::StockRoot;
use crate::workspace_home::WorkspaceHome;

/// What happened to one stock file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// It didn't exist; installed.
    Added,
    /// Unchanged since installed; replaced with the new version.
    Replaced,
    /// Unchanged since installed, and gone from the library: removed.
    Removed,
    /// Changed on both sides; merged without conflicts.
    Merged,
    /// Changed on both sides and the merge conflicts: the file is untouched, the merge with
    /// conflict markers is at `review`.
    Conflict { review: PathBuf },
    /// Changed, and there is no base to merge from (installed before bases were kept): the
    /// file is untouched, the new version is at `review`.
    Review { review: PathBuf },
}

/// One stock file that changed, by its path in the home folder
/// (`sites/home/index.md` for the library, `workspaces/default/sites/home/index.md` for a workspace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateItem {
    pub path: String,
    pub outcome: UpdateOutcome,
}

/// Updates the stock files (or, with `dry_run`, says what it would do). Files already up to
/// date, files the user deleted, and files only the user changed are left alone and not listed.
///
/// The library is committed before (whatever was edited by hand, or the whole folder when it
/// wasn't a repo yet) and after (the new stock), so its history tells the two apart.
pub async fn update_home(home: &Home, dry_run: bool) -> Result<Vec<UpdateItem>> {
    let library = Library::of(home);
    if !dry_run {
        let before = match library.head().map_err(ServerError::Invalid)? {
            None => "library: as it was before it became a git repo",
            Some(_) => "library: changes made before this update",
        };
        library.record(before).map_err(ServerError::Invalid)?;
    }
    let mut items = update_root(&StockRoot::library(home), dry_run).await?;
    if !dry_run {
        let recorded = library
            .record(&stock_message())
            .map_err(ServerError::Invalid)?;
        // Workspaces join at the latest stock commit: a library that never had one (it became a
        // repo already holding this stock) gets one that says so.
        let never = library
            .last_stock_commit()
            .map_err(ServerError::Invalid)?
            .is_none();
        if recorded.is_none() && never {
            library
                .mark(&stock_message())
                .map_err(ServerError::Invalid)?;
        }
    }
    for name in home.workspace_names()? {
        let workspace = home.workspace(&name);
        items.extend(update_workspace(&library, &workspace, dry_run).await?);
    }
    Ok(items)
}

/// A workspace's installed sites from the library. One whose stock sites came from the binary
/// (before the library) gets a last update from the binary, then joins: its stock sites are
/// recorded as installed from the library's latest stock commit.
async fn update_workspace(
    library: &Library,
    workspace: &WorkspaceHome,
    dry_run: bool,
) -> Result<Vec<UpdateItem>> {
    let mut items = Vec::new();
    let from_library = WorkspaceLibrary::of(workspace).map_err(ServerError::Invalid)?;
    if !workspace.library_file().exists() {
        items.extend(update_root(&StockRoot::workspace(workspace), dry_run).await?);
        let stock = library.last_stock_commit().map_err(ServerError::Invalid)?;
        if let (false, Some(commit)) = (dry_run, stock) {
            let names: Vec<SiteName> = stock_site_names()
                .iter()
                .filter_map(|n| SiteName::new(n).ok())
                .collect();
            from_library
                .adopt(&names, &commit)
                .map_err(ServerError::Invalid)?;
        }
    }
    let approvals = from_library.approvals().map_err(ServerError::Invalid)?;
    items.extend(
        from_library
            .update(dry_run, &approvals)
            .await
            .map_err(ServerError::Invalid)?,
    );
    Ok(items)
}

async fn update_root(stock: &StockRoot, dry_run: bool) -> Result<Vec<UpdateItem>> {
    let mut items = Vec::new();
    for (name, dir) in stock.trees() {
        for (rel, new) in files(dir) {
            let path = format!("{name}/{}", rel.display());
            if let Some(outcome) = update_file(stock, &path, new, dry_run)? {
                items.push(UpdateItem {
                    path: stock.label(&path),
                    outcome,
                });
            }
        }
    }
    let Some(approvals) = stock.approvals() else {
        return Ok(items);
    };
    if !dry_run {
        // Also the ones already up to date: they may predate approvals per caller.
        let stock_justfiles: Vec<String> = stock
            .trees()
            .iter()
            .flat_map(|(name, dir)| {
                files(dir)
                    .into_iter()
                    .map(move |(rel, new)| (stock.root().join(name).join(rel), new))
            })
            .filter(|(path, new)| {
                path.ends_with("justfile") && std::fs::read(path).ok().as_deref() == Some(*new)
            })
            .map(|(path, _)| path.display().to_string())
            .collect();
        approve_seed_recipes(approvals, &stock_justfiles).await?;
    }
    Ok(items)
}

/// Stock files whose new version isn't installed yet (what `update` would look at).
/// For a workspace that installs from the library: how many of its installed sites the library
/// is ahead of.
pub fn pending_updates(home: &Home) -> usize {
    let workspaces: usize = home
        .workspace_names()
        .unwrap_or_default()
        .iter()
        .map(|name| {
            let workspace = home.workspace(name);
            match WorkspaceLibrary::of(&workspace) {
                Ok(library) if workspace.library_file().exists() => library.behind(),
                _ => pending_in(&StockRoot::workspace(&workspace)),
            }
        })
        .sum();
    pending_in(&StockRoot::library(home)) + workspaces
}

fn pending_in(stock: &StockRoot) -> usize {
    stock
        .trees()
        .iter()
        .flat_map(|(name, dir)| {
            files(dir)
                .into_iter()
                .map(move |(rel, new)| (name, rel, new))
        })
        .filter(|(name, rel, new)| {
            let base = std::fs::read(stock.seed_base().join(name).join(rel)).ok();
            let current = std::fs::read(stock.root().join(name).join(rel)).ok();
            match (base, current) {
                (Some(base), _) => base != *new,
                (None, current) => current.as_deref() != Some(*new),
            }
        })
        .count()
}

fn files(dir: &'static Dir<'static>) -> Vec<(PathBuf, &'static [u8])> {
    let mut out: Vec<_> = dir
        .files()
        .map(|f| (f.path().to_path_buf(), f.contents()))
        .collect();
    for sub in dir.dirs() {
        out.extend(files(sub));
    }
    out
}

fn update_file(
    stock: &StockRoot,
    path: &str,
    new: &[u8],
    dry_run: bool,
) -> Result<Option<UpdateOutcome>> {
    let target = stock.root().join(path);
    let base_path = stock.seed_base().join(path);
    let base = read_optional(&base_path)?;
    let current = read_optional(&target)?;
    let review = stock.updates().join(path);
    let write = |path: &Path, content: &[u8]| {
        if dry_run {
            Ok(())
        } else {
            write_creating_dirs(path, content)
        }
    };

    let outcome = match (current, base) {
        // Deleted by the user: stays deleted. A new base keeps it from being reported again.
        (None, Some(base)) => {
            if base != new {
                write(&base_path, new)?;
            }
            return Ok(None);
        }
        (None, None) => {
            write(&target, new)?;
            UpdateOutcome::Added
        }
        (Some(current), base) if current == new => {
            if base.as_deref() != Some(new) {
                write(&base_path, new)?;
            }
            return Ok(None);
        }
        (Some(_), Some(base)) if base == new => return Ok(None), // only the user changed it
        (Some(current), Some(base)) if current == base => {
            write(&target, new)?;
            UpdateOutcome::Replaced
        }
        (Some(current), Some(base)) => match merge(stock, &current, &base, new)? {
            Merge::Clean(merged) => {
                write(&target, &merged)?;
                UpdateOutcome::Merged
            }
            Merge::Conflict(marked) => {
                write(&review, &marked)?;
                UpdateOutcome::Conflict { review }
            }
            Merge::Impossible => {
                write(&review, new)?;
                UpdateOutcome::Review { review }
            }
        },
        (Some(_), None) => {
            write(&review, new)?;
            UpdateOutcome::Review { review }
        }
    };
    write(&base_path, new)?;
    Ok(Some(outcome))
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ServerError::io(path, e)),
    }
}

pub(crate) enum Merge {
    Clean(Vec<u8>),
    /// With conflict markers.
    Conflict(Vec<u8>),
    /// Binary, or no `git` to merge with.
    Impossible,
}

/// Three-way merge of the user's version and the new one, from the installed base.
fn merge(stock: &StockRoot, current: &[u8], base: &[u8], new: &[u8]) -> Result<Merge> {
    merge_three(&stock.merge_scratch(), current, base, new)
}

/// Three-way merge (`git merge-file`) of `current` and `new` from `base`, using `scratch` (a
/// folder of its own, removed afterwards) for the three files.
pub(crate) fn merge_three(
    scratch: &Path,
    current: &[u8],
    base: &[u8],
    new: &[u8],
) -> Result<Merge> {
    let text = |b: &[u8]| std::str::from_utf8(b).is_ok() && !b.contains(&0);
    if !(text(current) && text(base) && text(new)) {
        return Ok(Merge::Impossible);
    }
    let scratch = scratch.to_path_buf();
    std::fs::create_dir_all(&scratch).map_err(|e| ServerError::io(&scratch, e))?;
    let paths = ["yours", "installed", "new"].map(|n| scratch.join(n));
    for (path, content) in paths.iter().zip([current, base, new]) {
        std::fs::write(path, content).map_err(|e| ServerError::io(path, e))?;
    }
    let output = Command::new("git")
        .args([
            "merge-file",
            "-p",
            "-L",
            "yours",
            "-L",
            "installed",
            "-L",
            "new",
        ])
        .args(&paths)
        .output();
    let _ = std::fs::remove_dir_all(&scratch);
    let Ok(output) = output else {
        return Ok(Merge::Impossible);
    };
    // Exit status: 0 clean, the number of conflicts (> 0), or negative/large on error.
    Ok(match output.status.code() {
        Some(0) => Merge::Clean(output.stdout),
        Some(n) if (1..=127).contains(&n) => Merge::Conflict(output.stdout),
        _ => Merge::Impossible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::{init_home, stock};
    use crate::workspace_home::WorkspaceHome;
    use crate::workspace_name::WorkspaceName;

    /// A fresh home and its default workspace, where the user's changes happen. The workspace is
    /// one from before the library (stock from the binary, `state/seed` bases): these tests cover
    /// that path, which such a workspace takes once before it joins the library.
    async fn fresh() -> (tempfile::TempDir, Home, WorkspaceHome) {
        let dir = tempfile::tempdir().unwrap();
        let home = Home::new(dir.path());
        init_home(&home).await.unwrap();
        let workspace = home.workspace(&WorkspaceName::default_workspace());
        std::fs::remove_dir_all(workspace.root()).unwrap();
        let standalone_then_moved = WorkspaceHome::new(dir.path().join("old"));
        crate::init::init_workspace(&standalone_then_moved)
            .await
            .unwrap();
        std::fs::rename(dir.path().join("old"), workspace.root()).unwrap();
        // A standalone workspace carries templates and a full config; this one wouldn't.
        let _ = std::fs::remove_dir_all(workspace.root().join("templates"));
        let _ = std::fs::remove_dir_all(workspace.seed_base().join("templates"));
        (dir, home, workspace)
    }

    fn page(home: &WorkspaceHome, path: &str) -> String {
        std::fs::read_to_string(home.root().join(path)).unwrap()
    }

    /// Pretends `path` was installed as `old` and the user's copy is `current`.
    fn installed(home: &WorkspaceHome, path: &str, old: &str, current: Option<&str>) {
        std::fs::write(home.seed_base().join(path), old).unwrap();
        match current {
            Some(text) => std::fs::write(home.root().join(path), text).unwrap(),
            None => std::fs::remove_file(home.root().join(path)).unwrap(),
        }
    }

    fn stock_text(path: &str) -> String {
        let (name, rel) = path.split_once('/').unwrap();
        let dir = stock().into_iter().find(|(n, _)| *n == name).unwrap().1;
        String::from_utf8(dir.get_file(rel).unwrap().contents().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn a_fresh_home_is_up_to_date() {
        let (_dir, home, _) = fresh().await;
        assert_eq!(pending_updates(&home), 0);
        assert!(update_home(&home, false).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn keeps_what_the_user_changed() {
        let (_dir, home, ws) = fresh().await;
        let new_home = stock_text("sites/home/index.md");
        let new_default = stock_text("sites/agents/default.md");
        let new_memory = stock_text("sites/memory/index.md");

        // Untouched by the user: replaced.
        installed(&ws, "sites/home/index.md", "old home\n", Some("old home\n"));
        // Changed by both, in different places: merged.
        let old_default = new_default.replacen("Be direct", "Be brief", 1);
        let mine = old_default.replacen("title: Default", "title: My default", 1);
        installed(&ws, "sites/agents/default.md", &old_default, Some(&mine));
        // Changed by both in the same place: conflict, untouched.
        let old_memory = new_memory.replacen("# Memory", "# Old memory", 1);
        let my_memory = new_memory.replacen("# Memory", "# My memory", 1);
        installed(&ws, "sites/memory/index.md", &old_memory, Some(&my_memory));
        // Deleted by the user: stays deleted.
        installed(&ws, "sites/plans/index.md", "old plans\n", None);
        // A new stock file.
        std::fs::remove_file(ws.root().join("sites/agents/worker.md")).unwrap();
        std::fs::remove_file(ws.seed_base().join("sites/agents/worker.md")).unwrap();
        // No base (an older install) and different: left for review.
        std::fs::remove_file(ws.seed_base().join("sites/agents/orchestrator.md")).unwrap();
        std::fs::write(ws.root().join("sites/agents/orchestrator.md"), "mine\n").unwrap();

        assert_eq!(pending_updates(&home), 6);
        let dry = update_home(&home, true).await.unwrap();
        assert_eq!(
            page(&ws, "sites/home/index.md"),
            "old home\n",
            "dry run writes nothing"
        );

        let items = update_home(&home, false).await.unwrap();
        assert_eq!(items, dry);
        let outcome = |path: &str| {
            items
                .iter()
                .find(|i| i.path == format!("workspaces/default/{path}"))
                .map(|i| i.outcome.clone())
        };
        assert_eq!(
            outcome("sites/home/index.md"),
            Some(UpdateOutcome::Replaced)
        );
        assert_eq!(page(&ws, "sites/home/index.md"), new_home);

        assert_eq!(
            outcome("sites/agents/default.md"),
            Some(UpdateOutcome::Merged)
        );
        let merged = page(&ws, "sites/agents/default.md");
        assert!(
            merged.contains("title: My default") && merged.contains("Be direct"),
            "{merged}"
        );

        let review = ws.updates().join("sites/memory/index.md");
        assert_eq!(
            outcome("sites/memory/index.md"),
            Some(UpdateOutcome::Conflict {
                review: review.clone()
            })
        );
        assert_eq!(page(&ws, "sites/memory/index.md"), my_memory);
        let marked = std::fs::read_to_string(&review).unwrap();
        assert!(
            marked.contains("<<<<<<< yours") && marked.contains(">>>>>>> new"),
            "{marked}"
        );

        assert_eq!(outcome("sites/plans/index.md"), None);
        assert!(!ws.root().join("sites/plans/index.md").exists());
        assert_eq!(
            outcome("sites/agents/worker.md"),
            Some(UpdateOutcome::Added)
        );
        assert!(matches!(
            outcome("sites/agents/orchestrator.md"),
            Some(UpdateOutcome::Review { .. })
        ));
        assert_eq!(page(&ws, "sites/agents/orchestrator.md"), "mine\n");

        // Reported once: what's left is in state/updates/.
        assert_eq!(pending_updates(&home), 0);
        assert!(update_home(&home, false).await.unwrap().is_empty());
    }
}
