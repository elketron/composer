//! How a site, work or stage may use its project.

use composer_core::MountMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectMode {
    /// Reads the project; writes through `project/` are refused.
    #[default]
    Read,
    /// Writes the project in place (assets, renders).
    Write,
    /// Writes the ticket's worktree.
    Worktree,
}

impl ProjectMode {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "write" => Self::Write,
            "worktree" => Self::Worktree,
            _ => Self::Read,
        }
    }

    pub fn readonly(self) -> bool {
        matches!(self, Self::Read)
    }

    pub fn from_mount(mode: MountMode) -> Self {
        match mode {
            MountMode::Read => Self::Read,
            MountMode::Write => Self::Write,
            MountMode::Worktree => Self::Worktree,
        }
    }

    pub fn as_mount(self) -> MountMode {
        match self {
            Self::Read => MountMode::Read,
            Self::Write => MountMode::Write,
            Self::Worktree => MountMode::Worktree,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_three_modes_and_defaults_to_read() {
        assert_eq!(ProjectMode::parse("write"), ProjectMode::Write);
        assert_eq!(ProjectMode::parse(" Worktree "), ProjectMode::Worktree);
        assert_eq!(ProjectMode::parse("read"), ProjectMode::Read);
        assert_eq!(ProjectMode::parse(""), ProjectMode::Read);
        assert_eq!(ProjectMode::default(), ProjectMode::Read);
        assert_eq!(ProjectMode::Read.as_mount(), MountMode::Read);
    }
}
