use std::path::PathBuf;

use composer_protocol::{Attachment, SessionInfo};
use serde_json::Value;

use crate::attachments::{AttachmentFolder, resolve};
use crate::workspace_home::WorkspaceHome;

/// A file a ticket refers to, checked.
#[derive(Debug, Clone)]
pub struct TicketFile {
    /// The session it was uploaded to.
    pub session: String,
    pub attachment: Attachment,
    pub path: PathBuf,
}

/// A ticket's `attachments:` (`<session>/<file>` refs, e.g. from the orchestrator's chat) that
/// belong to its project: uploaded to a session working in that same project (or, for a
/// ticket without one, a session without one). Others are left out and logged.
pub fn ticket_files(
    home: &WorkspaceHome,
    refs: Option<&Value>,
    project: Option<&str>,
) -> Vec<TicketFile> {
    let refs: Vec<&str> = match refs {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
        Some(Value::String(one)) => vec![one.as_str()],
        _ => vec![],
    };
    refs.into_iter()
        .filter_map(|reference| {
            let (session, file) = reference.trim().split_once('/')?;
            let folder = AttachmentFolder::of(home, session)
                .inspect_err(|e| tracing::warn!("ticket attachment {reference}: {e}"))
                .ok()?;
            let meta = std::fs::read_to_string(folder.session_meta()).ok()?;
            let info: SessionInfo = serde_json::from_str(&meta).ok()?;
            if info.project.as_deref() != project {
                tracing::warn!("ticket attachment {reference}: from another project, left out");
                return None;
            }
            let probe = Attachment {
                file: file.to_string(),
                name: String::new(),
                mime: String::new(),
                size: 0,
            };
            let (attachment, path) = resolve(home, session, &probe)
                .inspect_err(|e| tracing::warn!("ticket attachment {reference}: {e}"))
                .ok()?;
            Some(TicketFile {
                session: session.to_string(),
                attachment,
                path,
            })
        })
        .collect()
}
