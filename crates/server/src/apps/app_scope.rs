use std::fmt;

/// Where an app shows: inside a project, on the workspace's Home (no project open), or both (the
/// ticket board: everything on Home, the project's own inside one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScope {
    Project,
    Global,
    Both,
}

impl AppScope {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "project" => Ok(Self::Project),
            "global" => Ok(Self::Global),
            "both" => Ok(Self::Both),
            other => Err(format!(
                "`scope` is `project`, `global` or `both`; got `{other}`"
            )),
        }
    }

    /// It can attach to a project by its `when`.
    pub fn in_projects(self) -> bool {
        matches!(self, Self::Project | Self::Both)
    }

    /// It is on the workspace's Home.
    pub fn on_home(self) -> bool {
        matches!(self, Self::Global | Self::Both)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
            Self::Both => "both",
        }
    }
}

impl fmt::Display for AppScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
