use composer_protocol::ClientCommand;

/// The shown session's project, kept open by this connection for watches and `on_open`.
#[derive(Default)]
pub struct ProjectBinding {
    current: Option<String>,
}

impl ProjectBinding {
    /// A new socket owns no projects, even if the session has not changed.
    pub fn reset(&mut self) {
        self.current = None;
    }

    pub fn sync(&mut self, project: Option<&str>) -> Vec<ClientCommand> {
        if self.current.as_deref() == project {
            return vec![];
        }
        let mut commands = Vec::new();
        if let Some(project) = self.current.take() {
            commands.push(ClientCommand::ProjectClose { project });
        }
        self.current = project.map(str::to_string);
        if let Some(project) = &self.current {
            commands.push(ClientCommand::ProjectOpen {
                project: project.clone(),
            });
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_project_and_reopens_after_reconnecting() {
        let mut binding = ProjectBinding::default();
        let open = |p: &str| ClientCommand::ProjectOpen { project: p.into() };
        let close = |p: &str| ClientCommand::ProjectClose { project: p.into() };
        assert_eq!(binding.sync(Some("/projects/a")), [open("/projects/a")]);
        assert!(binding.sync(Some("/projects/a")).is_empty());
        assert_eq!(
            binding.sync(Some("/projects/b")),
            [close("/projects/a"), open("/projects/b")]
        );
        binding.reset();
        assert_eq!(binding.sync(Some("/projects/b")), [open("/projects/b")]);
        assert_eq!(binding.sync(None), [close("/projects/b")]);
        assert!(binding.sync(None).is_empty());
    }
}
