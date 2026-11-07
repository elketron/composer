use std::fmt;

/// How a publish changes one file of a library site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishChange {
    Added,
    Changed,
    Removed,
}

impl PublishChange {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Changed => "changed",
            Self::Removed => "removed",
        }
    }
}

impl fmt::Display for PublishChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}
