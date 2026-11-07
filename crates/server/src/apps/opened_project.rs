use crate::apps::ProjectWatch;

/// A project open in the workspace: how many clients have it, and its watch.
pub(crate) struct Opened {
    pub clients: u32,
    /// `None` when the watch couldn't start (reported; the project is open all the same).
    pub _watch: Option<ProjectWatch>,
}
