use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::content_hash;
use crate::mcp_bundles::{McpBundle, McpBundleManifest, slug};

/// Inside an unpacked folder: the SHA-256 of the `.mcpb` it came from.
pub(crate) const MARKER: &str = ".composer-bundle-sha256";

/// How large a bundle may be; one over any of these is skipped, with the limit in the warning.
/// Generous for real bundles (Blender's: 5.5 MB packed, 15.6 MB and 4,444 files unpacked), small
/// enough that a zip bomb cannot fill the disk or the memory.
#[derive(Debug, Clone, Copy)]
pub struct BundleLimits {
    /// The `.mcpb` file itself, in bytes.
    pub archive_bytes: u64,
    /// Entries in the archive (files and folders).
    pub entries: usize,
    /// One file, unpacked, in bytes.
    pub entry_bytes: u64,
    /// All files together, unpacked, in bytes.
    pub unpacked_bytes: u64,
}

const MIB: u64 = 1024 * 1024;

/// 256 MiB packed, 100,000 entries, 256 MiB per file and 1 GiB in all unpacked.
pub const BUNDLE_LIMITS: BundleLimits = BundleLimits {
    archive_bytes: 256 * MIB,
    entries: 100_000,
    entry_bytes: 256 * MIB,
    unpacked_bytes: 1024 * MIB,
};

/// `manifest.json` is read whole before anything is unpacked: at most this much of it.
const MANIFEST_BYTES: u64 = MIB;

/// A `.mcpb` file.
pub struct McpBundleFile {
    path: PathBuf,
}

impl McpBundleFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Reads the manifest, names the server and unpacks into `state/<name>/` unless that folder
    /// holds this very file already. Within [`BUNDLE_LIMITS`].
    pub fn load(&self, state: &Path) -> Result<McpBundle, String> {
        self.load_within(state, &BUNDLE_LIMITS)
    }

    /// [`Self::load`] with other limits. The file's size is checked before it is read, the
    /// number of entries before any is unpacked, and the unpacked sizes while the bytes are
    /// written (what the archive declares is not believed).
    pub fn load_within(&self, state: &Path, limits: &BundleLimits) -> Result<McpBundle, String> {
        let size = std::fs::metadata(&self.path)
            .map_err(|e| format!("cannot read it: {e}"))?
            .len();
        if size > limits.archive_bytes {
            return Err(format!(
                "the file is {size} bytes, over the limit of {} bytes for a bundle",
                limits.archive_bytes
            ));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&self.path)
            .map_err(|e| format!("cannot read it: {e}"))?
            .take(limits.archive_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read it: {e}"))?;
        if bytes.len() as u64 > limits.archive_bytes {
            return Err(format!(
                "the file is over the limit of {} bytes for a bundle",
                limits.archive_bytes
            ));
        }
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice()))
            .map_err(|e| format!("not a zip archive: {e}"))?;
        if archive.len() > limits.entries {
            return Err(format!(
                "it has {} entries, over the limit of {} for a bundle",
                archive.len(),
                limits.entries
            ));
        }
        let mut text = String::new();
        archive
            .by_name("manifest.json")
            .map_err(|_| "no manifest.json at the root of the archive".to_string())?
            .take(MANIFEST_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|e| format!("manifest.json is unreadable: {e}"))?;
        if text.len() as u64 > MANIFEST_BYTES {
            return Err(format!(
                "manifest.json is over the limit of {MANIFEST_BYTES} bytes"
            ));
        }
        let manifest = McpBundleManifest::parse(&text)?;
        let stem = self
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut name = slug(&manifest.name);
        if name.is_empty() {
            name = slug(&stem);
        }
        if name.is_empty() {
            return Err("no usable server name in the manifest or the file name".into());
        }
        std::fs::create_dir_all(state).map_err(|e| format!("{}: {e}", state.display()))?;
        let folder = std::path::absolute(state.join(&name))
            .map_err(|e| format!("{}: {e}", state.display()))?;
        let hash = content_hash(&bytes);
        if std::fs::read_to_string(folder.join(MARKER)).ok().as_deref() != Some(hash.as_str()) {
            unpack(&mut archive, &folder, &hash, limits)?;
        }
        Ok(McpBundle {
            name,
            manifest,
            folder,
        })
    }
}

/// `archive` into `target`, through a temporary sibling folder renamed into place (and removed
/// when the unpacking stops at a limit or an error: nothing half-unpacked stays).
fn unpack(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    target: &Path,
    hash: &str,
    limits: &BundleLimits,
) -> Result<(), String> {
    let mut unpacked: u64 = 0;
    let io = |e: std::io::Error| format!("unpacking into {}: {e}", target.display());
    let parent = target.parent().ok_or("no parent folder")?;
    let tmp = tempfile::Builder::new()
        .prefix(".unpack-")
        .tempdir_in(parent)
        .map_err(io)?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("unreadable entry: {e}"))?;
        let relative = entry.enclosed_name().ok_or_else(|| {
            format!(
                "entry `{}` would land outside the folder",
                String::from_utf8_lossy(entry.name_raw())
            )
        })?;
        if entry.is_symlink() {
            continue;
        }
        let out = tmp.path().join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(io)?;
            continue;
        }
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let mut file = std::fs::File::create(&out).map_err(io)?;
        // One byte more than either limit leaves: reaching it is the proof of being over.
        let room = limits
            .entry_bytes
            .min(limits.unpacked_bytes.saturating_sub(unpacked));
        let written = std::io::copy(&mut (&mut entry).take(room + 1), &mut file).map_err(io)?;
        if written > limits.entry_bytes {
            return Err(format!(
                "entry `{}` unpacks to over the limit of {} bytes for one file",
                relative.display(),
                limits.entry_bytes
            ));
        }
        unpacked += written;
        if unpacked > limits.unpacked_bytes {
            return Err(format!(
                "it unpacks to over the limit of {} bytes for a bundle",
                limits.unpacked_bytes
            ));
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            // The owner can always read and write what it unpacked.
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o777 | 0o600))
                .map_err(io)?;
        }
    }
    std::fs::write(tmp.path().join(MARKER), hash).map_err(io)?;
    if target.exists() {
        std::fs::remove_dir_all(target).map_err(io)?;
    }
    let ready = tmp.keep();
    if let Err(e) = std::fs::rename(&ready, target) {
        // Another process unpacked the same file meanwhile: keep its folder.
        let won = std::fs::read_to_string(target.join(MARKER)).ok().as_deref() == Some(hash);
        let _ = std::fs::remove_dir_all(&ready);
        if !won {
            return Err(io(e));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const MANIFEST: &str =
        r#"{ "name": "Blender", "server": { "mcp_config": { "command": "x" } } }"#;

    fn write_bundle(path: &Path, entries: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, content) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn unpacks_once_per_content() {
        let dir = tempfile::tempdir().unwrap();
        let (file, state) = (dir.path().join("b.mcpb"), dir.path().join("state"));
        write_bundle(&file, &[("manifest.json", MANIFEST), ("lib/a.txt", "one")]);
        let bundle = McpBundleFile::new(&file).load(&state).unwrap();
        assert_eq!(bundle.name, "blender");
        let a = bundle.folder.join("lib/a.txt");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "one");
        // Unchanged: the folder is left alone, even where someone edited it.
        std::fs::write(&a, "edited").unwrap();
        McpBundleFile::new(&file).load(&state).unwrap();
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "edited");
        // Changed: unpacked again, nothing of the old one left.
        write_bundle(&file, &[("manifest.json", MANIFEST), ("lib/b.txt", "two")]);
        McpBundleFile::new(&file).load(&state).unwrap();
        assert!(!a.exists());
        assert_eq!(
            std::fs::read_to_string(bundle.folder.join("lib/b.txt")).unwrap(),
            "two"
        );
        // Missing folder: unpacked again.
        std::fs::remove_dir_all(&bundle.folder).unwrap();
        McpBundleFile::new(&file).load(&state).unwrap();
        assert!(bundle.folder.join("lib/b.txt").exists());
    }

    #[test]
    fn refuses_entries_outside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (file, state) = (dir.path().join("b.mcpb"), dir.path().join("state"));
        write_bundle(&file, &[("manifest.json", MANIFEST), ("../escape", "x")]);
        let err = McpBundleFile::new(&file).load(&state).unwrap_err();
        assert!(err.contains("outside"), "{err}");
        assert!(!state.join("escape").exists() && !dir.path().join("escape").exists());
        assert!(!state.join("blender").exists());
    }

    #[test]
    fn bundles_over_a_limit_are_refused_and_leave_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (file, state) = (dir.path().join("b.mcpb"), dir.path().join("state"));
        let big = "x".repeat(4000);
        // Compressed, so the archive is small and the unpacked files are not.
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in [
            ("manifest.json", MANIFEST),
            ("a.txt", big.as_str()),
            ("b.txt", big.as_str()),
            ("c.txt", "small"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let size = std::fs::metadata(&file).unwrap().len();
        assert!(size < 2000, "{size}");
        let roomy = BundleLimits {
            archive_bytes: 10_000,
            entries: 4,
            entry_bytes: 4000,
            unpacked_bytes: 8200,
        };
        let refused = |limits: BundleLimits, says: &str| {
            let error = McpBundleFile::new(&file)
                .load_within(&state, &limits)
                .unwrap_err();
            assert!(error.contains(says) && error.contains("limit"), "{error}");
            let left: Vec<_> = std::fs::read_dir(&state)
                .map(|entries| entries.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            assert!(left.is_empty(), "{left:?}");
        };
        refused(
            BundleLimits {
                archive_bytes: size - 1,
                ..roomy
            },
            "the file is",
        );
        refused(
            BundleLimits {
                entries: 3,
                ..roomy
            },
            "4 entries",
        );
        refused(
            BundleLimits {
                entry_bytes: 3999,
                ..roomy
            },
            "entry `a.txt`",
        );
        refused(
            BundleLimits {
                unpacked_bytes: 7999,
                ..roomy
            },
            "it unpacks to over",
        );
        // Exactly at every limit: unpacked.
        let exact = BundleLimits {
            archive_bytes: size,
            unpacked_bytes: 8000 + 5 + MANIFEST.len() as u64,
            ..roomy
        };
        let bundle = McpBundleFile::new(&file)
            .load_within(&state, &exact)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(bundle.folder.join("b.txt")).unwrap(),
            big
        );
        // The real limits take a real bundle's size many times over.
        const {
            assert!(BUNDLE_LIMITS.archive_bytes >= 40 * 5_500_672);
            assert!(BUNDLE_LIMITS.unpacked_bytes >= 60 * 15_636_846);
            assert!(BUNDLE_LIMITS.entries >= 20 * 4444);
        }
    }

    #[test]
    fn unreadable_bundles_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let file = dir.path().join("x.mcpb");
        std::fs::write(&file, "not a zip").unwrap();
        assert!(McpBundleFile::new(&file).load(&state).is_err());
        write_bundle(&file, &[("other.json", "{}")]);
        assert!(McpBundleFile::new(&file).load(&state).is_err());
        write_bundle(&file, &[("manifest.json", "{")]);
        assert!(McpBundleFile::new(&file).load(&state).is_err());
        // No name in the manifest: the file stem.
        write_bundle(
            &file,
            &[(
                "manifest.json",
                r#"{"server":{"mcp_config":{"command":"x"}}}"#,
            )],
        );
        assert_eq!(McpBundleFile::new(&file).load(&state).unwrap().name, "x");
    }
}
