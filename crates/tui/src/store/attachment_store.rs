use composer_protocol::Attachment;

use crate::store::types::{PendingFile, UploadState};

/// Files attached to the message being written. Each uploads as soon as it is attached, so
/// sending only waits on what is still in flight.
#[derive(Debug, Default)]
pub struct AttachmentStore {
    pub files: Vec<PendingFile>,
    next: u64,
}

impl AttachmentStore {
    /// A new file, uploading to `session`; returns its id.
    pub fn add(&mut self, name: String, session: String) -> u64 {
        self.next += 1;
        self.files.push(PendingFile {
            id: self.next,
            name,
            session,
            state: UploadState::Uploading,
        });
        self.next
    }

    pub fn uploaded(&mut self, id: u64, result: Result<Attachment, String>) {
        if let Some(file) = self.files.iter_mut().find(|f| f.id == id) {
            file.state = match result {
                Ok(attachment) => {
                    file.name = attachment.name.clone();
                    UploadState::Ready(attachment)
                }
                Err(error) => UploadState::Failed(error),
            };
        }
    }

    pub fn remove_last(&mut self) -> Option<PendingFile> {
        self.files.pop()
    }

    pub fn clear(&mut self) {
        self.files.clear();
    }

    /// Why the message can't be sent yet, if it can't: an upload still running or failed.
    pub fn blocking(&self) -> Option<String> {
        self.files.iter().find_map(|f| match &f.state {
            UploadState::Uploading => Some(format!("{} is still uploading", f.name)),
            UploadState::Failed(e) => Some(format!(
                "{} failed to upload ({e}); backspace in an empty message removes it",
                f.name
            )),
            UploadState::Ready(_) => None,
        })
    }

    pub fn has_ready(&self) -> bool {
        self.files
            .iter()
            .any(|f| matches!(f.state, UploadState::Ready(_)))
    }

    /// The uploaded files of `session`, for the message; empties the list.
    pub fn take_ready(&mut self, session: &str) -> Vec<Attachment> {
        let files = std::mem::take(&mut self.files);
        files
            .into_iter()
            .filter(|f| f.session == session)
            .filter_map(|f| match f.state {
                UploadState::Ready(attachment) => Some(attachment),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(name: &str) -> Attachment {
        Attachment {
            file: format!("abc-{name}"),
            name: name.into(),
            mime: "text/plain".into(),
            size: 1,
        }
    }

    #[test]
    fn upload_then_send() {
        let mut store = AttachmentStore::default();
        let a = store.add("a.txt".into(), "s1".into());
        let b = store.add("b.png".into(), "s1".into());
        assert!(
            store
                .blocking()
                .unwrap()
                .contains("a.txt is still uploading")
        );
        store.uploaded(a, Ok(attachment("a.txt")));
        store.uploaded(b, Err("too big".into()));
        assert!(
            store
                .blocking()
                .unwrap()
                .contains("b.png failed to upload (too big)")
        );
        store.remove_last();
        assert_eq!(store.blocking(), None);
        assert_eq!(store.take_ready("s1"), vec![attachment("a.txt")]);
        assert!(store.files.is_empty());
    }
}
