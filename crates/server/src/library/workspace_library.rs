use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::actions::Approvals;
use crate::home::Home;
use crate::init::{approve_recipes_in, stock_file, write_creating_dirs};
use crate::library::{Installed, InstalledSites, Library, LibraryCommit, SiteName};
use crate::update::{Merge, UpdateItem, UpdateOutcome, merge_three};
use crate::workspace_home::WorkspaceHome;

/// One workspace seen from the library: installing library sites into it, and updating what it
/// installed. Reads the library; writes only into this workspace (its `sites/` and `state/`).
#[derive(Debug, Clone)]
pub struct WorkspaceLibrary {
    library: Library,
    workspace: WorkspaceHome,
}

/// A library site, and how it stands in this workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySiteState {
    pub site: SiteName,
    /// The last commit that changed it.
    pub commit: LibraryCommit,
    /// This workspace's sites installed from it, each with whether the library is ahead of it.
    pub installed: Vec<(SiteName, bool)>,
}

/// What an install did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub site: SiteName,
    pub from: SiteName,
    pub commit: LibraryCommit,
    pub files: usize,
    /// It has a `ui/`: built with its `ui-build` action (asked for like any recipe), not here.
    pub has_ui: bool,
}

impl WorkspaceLibrary {
    /// The library of `workspace`'s home. A standalone workspace (its own home) has none.
    pub fn of(workspace: &WorkspaceHome) -> Result<Self, String> {
        if workspace.is_standalone() {
            return Err(format!(
                "{} is a standalone workspace: it has no library",
                workspace.root().display()
            ));
        }
        Ok(Self {
            library: Library::of(&Home::new(workspace.home_root())),
            workspace: workspace.clone(),
        })
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    /// The workspace's approvals as stored (for callers outside a running server).
    pub fn approvals(&self) -> Result<Approvals, String> {
        Approvals::load(&self.workspace.approvals_file()).map_err(|e| e.to_string())
    }

    pub fn installed(&self) -> Result<InstalledSites, String> {
        InstalledSites::load(&self.workspace.library_file())
    }

    /// Every library site, with what this workspace installed from it.
    pub fn list(&self) -> Result<Vec<LibrarySiteState>, String> {
        let installed = self.installed()?;
        self.library
            .sites()?
            .into_iter()
            .map(|(name, commit)| {
                let site = SiteName::new(&name)?;
                let mine = installed
                    .iter()
                    .filter(|(_, from)| from.from == site)
                    .map(|(local, from)| (local.clone(), self.is_behind(from)))
                    .collect();
                Ok(LibrarySiteState {
                    site,
                    commit,
                    installed: mine,
                })
            })
            .collect()
    }

    /// Copies library site `site` (at the library's HEAD) into this workspace as `name` (default:
    /// the same name), and records where it came from. Refuses a name the workspace already has.
    /// A justfile identical to the binary's stock one keeps its recipes pre-approved; any other
    /// asks for approval on first use, as every new recipe version does.
    pub async fn install(
        &self,
        site: &SiteName,
        name: Option<&SiteName>,
        approvals: &Approvals,
    ) -> Result<Installation, String> {
        let name = name.unwrap_or(site);
        let dest = self.workspace.sites().join(name.as_str());
        if dest.exists() {
            return Err(format!(
                "this workspace already has a site `{name}`: install it under another name \
                 (`--as <name>`), or update it with `composer library update`"
            ));
        }
        let head = self.library.head()?.ok_or("the library is empty")?;
        let files = self.library.files_at(&head, site.as_str())?;
        if files.is_empty() {
            return Err(format!(
                "the library has no site `{site}`; `composer library list` shows what it has"
            ));
        }
        for rel in &files {
            let bytes = self
                .library
                .read_at(&head, site.as_str(), rel)?
                .ok_or_else(|| format!("{site}/{} vanished from the library", rel.display()))?;
            write_creating_dirs(&dest.join(rel), &bytes).map_err(|e| e.to_string())?;
        }
        self.approve_stock_recipes(approvals, site, &[dest.join("justfile")])
            .await?;
        self.installed()?.set(
            name.clone(),
            Installed {
                from: site.clone(),
                commit: head.clone(),
            },
        )?;
        Ok(Installation {
            site: name.clone(),
            from: site.clone(),
            commit: head,
            files: files.len(),
            has_ui: files.iter().any(|f| f.starts_with("ui")),
        })
    }

    /// Records `sites` (already in the workspace) as installed from the same library sites at
    /// `commit`: how a workspace's stock sites from before the library join it.
    pub fn adopt(&self, sites: &[SiteName], commit: &LibraryCommit) -> Result<(), String> {
        let mut installed = self.installed()?;
        for site in sites {
            if installed.get(site).is_none() && self.workspace.sites().join(site.as_str()).is_dir()
            {
                let from = Installed {
                    from: site.clone(),
                    commit: commit.clone(),
                };
                installed.set(site.clone(), from)?;
            }
        }
        Ok(())
    }

    /// Brings every installed site up to the library's HEAD, per file: unchanged here → replaced;
    /// changed on both sides → three-way merge; a conflict, or no common base, leaves the
    /// workspace file alone and puts the library's version in `state/updates/`. With `dry_run`,
    /// says what it would do and writes nothing.
    pub async fn update(
        &self,
        dry_run: bool,
        approvals: &Approvals,
    ) -> Result<Vec<UpdateItem>, String> {
        let Some(head) = self.library.head()? else {
            return Ok(Vec::new());
        };
        let mut installed = self.installed()?;
        let behind: Vec<(SiteName, Installed)> = installed
            .iter()
            .filter(|(_, from)| from.commit != head)
            .map(|(site, from)| (site.clone(), from.clone()))
            .collect();
        let mut items = Vec::new();
        if !dry_run {
            // Also the ones up to date: approvals may predate approvals per caller.
            for (site, from) in installed.iter() {
                let justfile = self.workspace.sites().join(site.as_str()).join("justfile");
                self.approve_stock_recipes(approvals, &from.from, &[justfile])
                    .await?;
            }
        }
        for (site, from) in behind {
            items.extend(
                self.update_site(&site, &from, &head, dry_run, approvals)
                    .await?,
            );
            if !dry_run {
                let moved = Installed {
                    from: from.from.clone(),
                    commit: head.clone(),
                };
                installed.set(site, moved)?;
            }
        }
        Ok(items)
    }

    /// How many installed sites the library changed since they were installed or updated.
    pub fn behind(&self) -> usize {
        let Ok(installed) = self.installed() else {
            return 0;
        };
        installed
            .iter()
            .filter(|(_, from)| self.is_behind(from))
            .count()
    }

    /// Whether the library changed the site `from` names since its recorded commit (a commit the
    /// library lost counts as behind: its update says what to do).
    fn is_behind(&self, from: &Installed) -> bool {
        self.library
            .changed_since(&from.commit, from.from.as_str())
            .unwrap_or(true)
    }

    async fn update_site(
        &self,
        site: &SiteName,
        from: &Installed,
        head: &LibraryCommit,
        dry_run: bool,
        approvals: &Approvals,
    ) -> Result<Vec<UpdateItem>, String> {
        if !self.library.has(&from.commit)? {
            return Err(format!(
                "workspace {}'s site `{site}` was installed from library commit {}, which the \
                 library no longer has (was its history rewritten?). Remove `{site}` from {} and \
                 install it again, or delete that file and run `composer update` to record the \
                 stock sites afresh",
                self.workspace.name(),
                from.commit.short(),
                self.workspace.library_file().display()
            ));
        }
        let library_site = from.from.as_str();
        let mut paths: BTreeSet<PathBuf> = self
            .library
            .files_at(&from.commit, library_site)?
            .into_iter()
            .collect();
        paths.extend(self.library.files_at(head, library_site)?);
        let mut items = Vec::new();
        let mut justfiles = Vec::new();
        for rel in paths {
            let base = self.library.read_at(&from.commit, library_site, &rel)?;
            let new = self.library.read_at(head, library_site, &rel)?;
            let target = self.workspace.sites().join(site.as_str()).join(&rel);
            let Some(outcome) = self.update_file(site, &rel, base, new, dry_run)? else {
                continue;
            };
            if rel == Path::new("justfile") {
                justfiles.push(target);
            }
            items.push(UpdateItem {
                path: self.label(site, &rel),
                outcome,
            });
        }
        if !dry_run {
            self.approve_stock_recipes(approvals, &from.from, &justfiles)
                .await?;
        }
        Ok(items)
    }

    /// One file of an installed site: `base` as installed, `new` as the library has it now.
    fn update_file(
        &self,
        site: &SiteName,
        rel: &Path,
        base: Option<Vec<u8>>,
        new: Option<Vec<u8>>,
        dry_run: bool,
    ) -> Result<Option<UpdateOutcome>, String> {
        if base == new {
            return Ok(None);
        }
        let target = self.workspace.sites().join(site.as_str()).join(rel);
        let current = read_optional(&target)?;
        if current == new {
            return Ok(None);
        }
        let review = self
            .workspace
            .updates()
            .join("sites")
            .join(site.as_str())
            .join(rel);
        let write = |path: &Path, bytes: &[u8]| -> Result<(), String> {
            if dry_run {
                return Ok(());
            }
            write_creating_dirs(path, bytes).map_err(|e| e.to_string())
        };
        let outcome = match (current, base, new) {
            // Deleted here: stays deleted.
            (None, Some(_), _) => return Ok(None),
            (None, None, Some(new)) => {
                write(&target, &new)?;
                UpdateOutcome::Added
            }
            (Some(current), Some(base), None) if current == base => {
                if !dry_run {
                    std::fs::remove_file(&target).map_err(|e| e.to_string())?;
                }
                UpdateOutcome::Removed
            }
            // Gone from the library but changed here: kept, not reported again.
            (Some(_), _, None) => return Ok(None),
            (Some(current), Some(base), Some(new)) if current == base => {
                write(&target, &new)?;
                UpdateOutcome::Replaced
            }
            (Some(current), Some(base), Some(new)) => {
                let scratch = self.workspace.state().join("merge");
                match merge_three(&scratch, &current, &base, &new).map_err(|e| e.to_string())? {
                    Merge::Clean(merged) => {
                        write(&target, &merged)?;
                        UpdateOutcome::Merged
                    }
                    Merge::Conflict(marked) => {
                        write(&review, &marked)?;
                        UpdateOutcome::Conflict { review }
                    }
                    Merge::Impossible => {
                        write(&review, &new)?;
                        UpdateOutcome::Review { review }
                    }
                }
            }
            (Some(_), None, Some(new)) => {
                write(&review, &new)?;
                UpdateOutcome::Review { review }
            }
            (None, None, None) => return Ok(None),
        };
        Ok(Some(outcome))
    }

    /// Pre-approves the recipes of those `justfiles` that are exactly the binary's stock ones for
    /// library site `site`.
    async fn approve_stock_recipes(
        &self,
        approvals: &Approvals,
        site: &SiteName,
        justfiles: &[PathBuf],
    ) -> Result<(), String> {
        let stock: Vec<String> = justfiles
            .iter()
            .filter(|path| {
                let rel = Path::new(site.as_str()).join("justfile");
                let current = std::fs::read(path).ok();
                current.is_some() && current.as_deref() == stock_file(&rel)
            })
            .map(|path| path.display().to_string())
            .collect();
        approve_recipes_in(approvals, &stock)
            .await
            .map_err(|e| e.to_string())
    }

    /// `workspaces/<ws>/sites/<site>/<rel>`, as `composer update` reports paths.
    fn label(&self, site: &SiteName, rel: &Path) -> String {
        format!(
            "workspaces/{}/sites/{site}/{}",
            self.workspace.name(),
            rel.display()
        )
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
