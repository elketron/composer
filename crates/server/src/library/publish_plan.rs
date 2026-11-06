use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hash::content_hash;
use crate::library::{InstalledSites, Library, SiteDefinition, SiteName};

/// What publishing a workspace site to the library would do, for the user to approve: the
/// definition's files, what changes in the library, what of the site stays out (works, tickets,
/// pages), and a digest of exactly those contents, so a publish only goes ahead with what was
/// approved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPlan {
    /// The workspace's site.
    pub site: SiteName,
    /// Its name in the library.
    pub name: SiteName,
    /// The library has no such site yet.
    pub new_site: bool,
    /// Every file of the definition, relative to the site.
    pub files: Vec<PathBuf>,
    /// Per file that differs from the library: `added`, `changed` or `removed`.
    pub changes: Vec<(PathBuf, &'static str)>,
    /// Top-level entries of the site that are not published.
    pub left_out: Vec<PathBuf>,
    /// `library:` entries that don't exist.
    pub missing: Vec<PathBuf>,
    /// The changes as a unified diff (cut when long).
    pub diff: String,
    /// The definition's paths and contents, hashed.
    pub digest: String,
}

/// Longer diffs are cut, with a note.
const MAX_DIFF: usize = 20_000;

impl PublishPlan {
    /// The plan for workspace site `dir` (named `site`) going to library site `name`. Refused when
    /// the library has `name` and this workspace's copy isn't installed from it, or the library
    /// changed it since this copy's commit (update first: never overwrite another workspace's
    /// publish unseen).
    pub fn make(
        library: &Library,
        installed: &InstalledSites,
        site: &SiteName,
        dir: &Path,
        name: &SiteName,
    ) -> Result<Self, String> {
        if !dir.is_dir() {
            return Err(format!("this workspace has no site `{site}`"));
        }
        let definition = SiteDefinition::read(dir)?;
        let head = library.head()?;
        let in_library = match &head {
            Some(head) => library.files_at(head, name.as_str())?,
            None => Vec::new(),
        };
        let new_site = in_library.is_empty();
        if !new_site {
            check_in_step(library, installed, site, name)?;
        }
        let mut changes = Vec::new();
        let mut diff = String::new();
        let paths: BTreeSet<&PathBuf> = definition.files().iter().chain(&in_library).collect();
        for rel in paths {
            let ours = definition
                .files()
                .contains(rel)
                .then(|| std::fs::read(dir.join(rel)).ok())
                .flatten();
            let theirs = match &head {
                Some(head) if in_library.contains(rel) => {
                    library.read_at(head, name.as_str(), rel)?
                }
                _ => None,
            };
            let kind = match (&theirs, &ours) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(a), Some(b)) if a != b => "changed",
                _ => continue,
            };
            changes.push((rel.clone(), kind));
            if diff.len() < MAX_DIFF {
                diff.push_str(&file_diff(rel, theirs.as_deref(), ours.as_deref()));
            }
        }
        if diff.len() > MAX_DIFF {
            diff.truncate(diff.floor_char_boundary(MAX_DIFF));
            diff.push_str("\n… (diff cut; the file list above is complete)\n");
        }
        Ok(Self {
            site: site.clone(),
            name: name.clone(),
            new_site,
            files: definition.files().iter().cloned().collect(),
            changes,
            left_out: definition.left_out().to_vec(),
            missing: definition.missing().to_vec(),
            diff,
            digest: digest(dir, definition.files()),
        })
    }

    /// The plan as the user reads it before approving.
    pub fn describe(&self) -> String {
        let list = |paths: &[PathBuf]| {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut text = match self.new_site {
            true => format!(
                "Publish site://{} to the library as a new site `{}` ({} files).\n",
                self.site,
                self.name,
                self.files.len()
            ),
            false => format!(
                "Publish site://{} to the library's `{}`: {} file(s) change.\n",
                self.site,
                self.name,
                self.changes.len()
            ),
        };
        for (path, kind) in &self.changes {
            text.push_str(&format!("  {kind:<8} {}\n", path.display()));
        }
        if !self.left_out.is_empty() {
            text.push_str(&format!(
                "Not published (content, not definition): {}\n",
                list(&self.left_out)
            ));
        }
        if !self.missing.is_empty() {
            text.push_str(&format!(
                "Listed under `library:` but missing: {}\n",
                list(&self.missing)
            ));
        }
        if !self.diff.is_empty() {
            text.push('\n');
            text.push_str(&self.diff);
        }
        text
    }
}

/// The library's `name` may only be published over from a copy installed from it, and only one
/// that is up to date with it.
fn check_in_step(
    library: &Library,
    installed: &InstalledSites,
    site: &SiteName,
    name: &SiteName,
) -> Result<(), String> {
    let Some(from) = installed.get(site).filter(|from| &from.from == name) else {
        return Err(format!(
            "the library already has a site `{name}`, and this workspace's `{site}` wasn't \
             installed from it: publish under another name (`--as <name>`), or install the \
             library's `{name}` and work from that"
        ));
    };
    if library.changed_since(&from.commit, name.as_str())? {
        return Err(format!(
            "the library's `{name}` changed since this workspace installed or published it (at \
             {}): run `composer library update` first, look at what came in, then publish",
            from.commit.short()
        ));
    }
    Ok(())
}

/// The definition's paths and contents, as one hash.
fn digest(dir: &Path, files: &BTreeSet<PathBuf>) -> String {
    let mut all = Vec::new();
    for rel in files {
        all.extend(rel.display().to_string().as_bytes());
        all.push(0);
        all.extend(std::fs::read(dir.join(rel)).unwrap_or_default());
        all.push(0);
    }
    content_hash(&all)
}

/// A unified diff of one file, library version → workspace version (`git diff --no-index`).
fn file_diff(rel: &Path, old: Option<&[u8]>, new: Option<&[u8]>) -> String {
    let text = |b: Option<&[u8]>| b.is_none_or(|b| std::str::from_utf8(b).is_ok());
    if !(text(old) && text(new)) {
        return format!("Binary file {} differs\n", rel.display());
    }
    let Ok(scratch) = tempfile::tempdir() else {
        return String::new();
    };
    let write = |name: &str, bytes: Option<&[u8]>| {
        let path = scratch.path().join(name);
        std::fs::write(&path, bytes.unwrap_or_default()).map(|()| path)
    };
    let (Ok(a), Ok(b)) = (write("library", old), write("workspace", new)) else {
        return String::new();
    };
    let output = Command::new("git")
        .args(["diff", "--no-index", "--no-color", "-U3"])
        .arg(&a)
        .arg(&b)
        .output();
    let Ok(output) = output else {
        return String::new();
    };
    let body = String::from_utf8_lossy(&output.stdout);
    // Name the file, not the scratch copies.
    let body = body
        .lines()
        .skip_while(|line| !line.starts_with("@@"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "--- library/{0}\n+++ workspace/{0}\n{body}\n",
        rel.display()
    )
}
