use composer_protocol::Attachment;

/// A file going with the next message: uploading, ready, or refused.
#[derive(Debug, Clone, PartialEq)]
pub enum Upload {
    Uploading {
        id: u64,
        session: String,
        name: String,
    },
    Ready(Attachment),
    Failed(String, String),
}

/// Draft attachments. IDs are never reused, including after a send or a session switch.
#[derive(Default)]
pub struct Uploads {
    entries: Vec<Upload>,
    next: u64,
}

impl Uploads {
    pub fn start(&mut self, session: &str, name: String) -> u64 {
        let id = self.next;
        self.next += 1;
        self.entries.push(Upload::Uploading {
            id,
            session: session.to_string(),
            name,
        });
        id
    }

    pub fn complete(&mut self, session: &str, id: u64, result: Result<Attachment, String>) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| {
            matches!(entry, Upload::Uploading { id: pending, session: owner, .. }
                if *pending == id && owner == session)
        }) else {
            return false;
        };
        let Upload::Uploading { name, .. } = entry else {
            unreachable!()
        };
        *entry = match result {
            Ok(file) => Upload::Ready(file),
            Err(error) => Upload::Failed(name.clone(), error),
        };
        true
    }

    /// Failed files must be removed explicitly, not silently left out of a message.
    pub fn ready(&self) -> bool {
        self.entries
            .iter()
            .all(|entry| matches!(entry, Upload::Ready(_)))
    }

    pub fn take(&mut self) -> Option<Vec<Attachment>> {
        if !self.ready() {
            return None;
        }
        Some(
            std::mem::take(&mut self.entries)
                .into_iter()
                .map(|entry| {
                    let Upload::Ready(file) = entry else {
                        unreachable!()
                    };
                    file
                })
                .collect(),
        )
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Upload> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> Attachment {
        Attachment {
            file: format!("hash-{name}"),
            name: name.into(),
            mime: "text/plain".into(),
            size: 5,
        }
    }

    #[test]
    fn a_send_keeps_uploading_and_failed_files_until_handled() {
        let mut uploads = Uploads::default();
        let first = uploads.start("a", "one.txt".into());
        let second = uploads.start("a", "two.txt".into());
        uploads.complete("a", first, Ok(file("one.txt")));
        assert!(!uploads.ready());
        assert_eq!(uploads.take(), None);
        assert_eq!(uploads.len(), 2);
        uploads.complete("a", second, Err("offline".into()));
        assert_eq!(uploads.take(), None);
        uploads.remove(1);
        assert_eq!(uploads.take(), Some(vec![file("one.txt")]));
        assert!(uploads.is_empty());
    }

    #[test]
    fn completions_belong_to_a_session_and_a_unique_upload_not_a_filename() {
        let mut uploads = Uploads::default();
        let old = uploads.start("a", "notes.txt".into());
        uploads.clear();
        let current = uploads.start("b", "notes.txt".into());
        assert_ne!(old, current);
        assert!(!uploads.complete("a", old, Ok(file("old.txt"))));
        assert!(!uploads.complete("a", current, Ok(file("wrong-session.txt"))));
        assert!(!uploads.ready());
        assert!(uploads.complete("b", current, Ok(file("new.txt"))));
        assert_eq!(uploads.take(), Some(vec![file("new.txt")]));

        let removed = uploads.start("b", "notes.txt".into());
        uploads.remove(0);
        let replacement = uploads.start("b", "notes.txt".into());
        assert!(!uploads.complete("b", removed, Ok(file("removed.txt"))));
        assert!(uploads.complete("b", replacement, Ok(file("replacement.txt"))));
        assert_eq!(uploads.take(), Some(vec![file("replacement.txt")]));
    }
}
