use std::ffi::OsString;
use std::path::PathBuf;

/// One mount of a sandbox. Paths are the same inside as outside (so paths in pages stay true),
/// except for [`Mount::RoAt`]. Binds of paths that don't exist are skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mount {
    /// Read-only.
    Ro(PathBuf),
    /// Read-write.
    Rw(PathBuf),
    /// A device node or folder of them (`/dev/dri`).
    Dev(PathBuf),
    /// `outside`, read-only, at `inside`.
    RoAt { outside: PathBuf, inside: PathBuf },
    /// `outside`, read-write, at `inside`.
    RwAt { outside: PathBuf, inside: PathBuf },
    /// An empty folder over what is there: hides it.
    Hide(PathBuf),
}

impl Mount {
    /// The `bwrap` arguments for it.
    pub fn args(&self) -> Vec<OsString> {
        let two = |flag: &str, path: &PathBuf| vec![flag.into(), path.into(), path.into()];
        match self {
            Self::Ro(path) => two("--ro-bind-try", path),
            Self::Rw(path) => two("--bind-try", path),
            Self::Dev(path) => two("--dev-bind-try", path),
            Self::RoAt { outside, inside } => {
                vec!["--ro-bind-try".into(), outside.into(), inside.into()]
            }
            Self::RwAt { outside, inside } => {
                vec!["--bind-try".into(), outside.into(), inside.into()]
            }
            Self::Hide(path) => vec!["--tmpfs".into(), path.into()],
        }
    }

    /// The path it makes visible inside, if any.
    pub fn inside(&self) -> Option<&PathBuf> {
        match self {
            Self::Ro(path) | Self::Rw(path) | Self::Dev(path) => Some(path),
            Self::RoAt { inside, .. } | Self::RwAt { inside, .. } => Some(inside),
            Self::Hide(_) => None,
        }
    }

    /// Whether it can be written through.
    pub fn writable(&self) -> bool {
        matches!(self, Self::Rw(_) | Self::Dev(_) | Self::RwAt { .. })
    }
}
