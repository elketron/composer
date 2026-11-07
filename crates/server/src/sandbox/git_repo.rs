use std::path::{Path, PathBuf};

use crate::sandbox::sandbox_spec::canonical;

/// The repository a folder is in, found by walking up to its `.git`, without trusting what a
/// sandboxed run could have written there: a linked worktree's `.git` file counts only when the
/// repository it names points back to it. (A run can rewrite its worktree's `.git` file; it can't
/// write another repository's `worktrees/<name>/gitdir`.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRepo {
    /// A checkout with its own `.git` folder (or a bare repository).
    Checkout(PathBuf),
    /// A linked worktree and the main repository's `.git` it belongs to.
    Worktree { dir: PathBuf, common: PathBuf },
}

impl GitRepo {
    /// The repository `dir` is in; `None` when it isn't in one. An `Err` for a `.git` that is a
    /// symlink or a worktree whose repository doesn't point back.
    pub fn discover(dir: &Path) -> Result<Option<Self>, String> {
        let start = canonical(dir);
        for top in start.ancestors() {
            let dot = top.join(".git");
            let Ok(meta) = std::fs::symlink_metadata(&dot) else {
                if top.join("HEAD").is_file() && top.join("objects").is_dir() {
                    return Ok(Some(Self::Checkout(top.to_path_buf())));
                }
                continue;
            };
            if meta.is_dir() {
                return Ok(Some(Self::Checkout(top.to_path_buf())));
            }
            if meta.is_file() {
                return Self::worktree(top, &dot).map(Some);
            }
            return Err(format!(
                "{}: `.git` is a symlink; composer doesn't follow it",
                dot.display()
            ));
        }
        Ok(None)
    }

    /// `dir`'s `.git` file names `<common>/worktrees/<name>`, whose `gitdir` names it back.
    fn worktree(dir: &Path, dot: &Path) -> Result<Self, String> {
        let refuse = || {
            format!(
                "{}: its `.git` file doesn't name a worktree of a repository that names it back; \
                 run `git worktree repair {}` in the main checkout",
                dir.display(),
                dir.display()
            )
        };
        let text = std::fs::read_to_string(dot).map_err(|_| refuse())?;
        let named = PathBuf::from(
            text.trim()
                .strip_prefix("gitdir:")
                .ok_or_else(refuse)?
                .trim(),
        );
        let gitdir = canonical(&dir.join(named));
        let common = gitdir
            .parent()
            .filter(|w| w.file_name().is_some_and(|n| n == "worktrees"))
            .and_then(Path::parent)
            .ok_or_else(refuse)?;
        let back = std::fs::read_to_string(gitdir.join("gitdir")).map_err(|_| refuse())?;
        let points_back = canonical(Path::new(back.trim())) == canonical(dot);
        if !points_back || !common.join("HEAD").is_file() {
            return Err(refuse());
        }
        Ok(Self::Worktree {
            dir: dir.to_path_buf(),
            common: common.to_path_buf(),
        })
    }

    /// The repository's `.git` (the main one, for a worktree).
    fn common(&self) -> PathBuf {
        match self {
            Self::Checkout(top) if top.join(".git").is_dir() => top.join(".git"),
            Self::Checkout(bare) => bare.clone(),
            Self::Worktree { common, .. } => common.clone(),
        }
    }

    /// Every linked worktree of the repository that names it back (so git sees them where they
    /// are): what `<.git>/worktrees/*/gitdir` lists, checked like [`Self::discover`].
    pub fn worktrees(&self) -> Vec<PathBuf> {
        let common = self.common();
        let listed = std::fs::read_dir(common.join("worktrees"))
            .into_iter()
            .flatten();
        listed
            .flatten()
            .filter_map(|entry| std::fs::read_to_string(entry.path().join("gitdir")).ok())
            .filter_map(|dot| Path::new(dot.trim()).parent().map(Path::to_path_buf))
            .filter(|dir| {
                matches!(Self::discover(dir), Ok(Some(Self::Worktree { common: c, .. })) if c == common)
            })
            .collect()
    }

    /// What git on it writes: the checkout (with its `.git`), or the worktree and the main `.git`
    /// (not the main checkout).
    pub fn writable(&self) -> Vec<PathBuf> {
        match self {
            Self::Checkout(top) => vec![top.clone()],
            Self::Worktree { dir, common } => vec![dir.clone(), common.clone()],
        }
    }
}
