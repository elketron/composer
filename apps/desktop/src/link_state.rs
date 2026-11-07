/// How the window's connection is doing, for the title bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Up,
    /// Lost (or never made); it keeps retrying.
    Down(String),
    /// Connected, but the last request failed.
    Refused(String),
}

impl LinkState {
    /// What the title bar adds after the workspace and project ("" when all is well).
    pub fn suffix(&self) -> String {
        match self {
            Self::Up => String::new(),
            Self::Connecting => "   connecting…".into(),
            Self::Down(reason) => format!("   ⚠ {reason} · retrying"),
            Self::Refused(error) => format!("   ⚠ {error}"),
        }
    }

    /// A request failed: shown while connected; a lost connection says more.
    pub fn refused(&self, error: String) -> Self {
        match self {
            Self::Up | Self::Refused(_) => Self::Refused(error),
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_and_refusals() {
        assert_eq!(LinkState::Up.suffix(), "");
        assert!(
            LinkState::Down("can't connect".into())
                .suffix()
                .contains("retrying")
        );
        assert_eq!(
            LinkState::Up.refused("no".into()),
            LinkState::Refused("no".into())
        );
        let down = LinkState::Down("gone".into());
        assert_eq!(
            down.refused("no".into()),
            down,
            "the connection's state wins"
        );
    }
}
