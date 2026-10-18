use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Result, ServerError};
use crate::provider::ModelMessage;

/// The conversation as the model sees it (`history.jsonl`), excluding the system prompt.
#[derive(Debug)]
pub struct History {
    path: PathBuf,
    messages: Vec<ModelMessage>,
}

impl History {
    pub fn load(path: &Path) -> Result<Self> {
        let messages = match std::fs::read_to_string(path) {
            Ok(text) => text
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(ServerError::io(path, e)),
        };
        Ok(Self {
            path: path.to_path_buf(),
            messages,
        })
    }

    pub fn push(&mut self, message: ModelMessage) -> Result<()> {
        let line = serde_json::to_string(&message).expect("messages serialize");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| ServerError::io(&self.path, e))?;
        writeln!(file, "{line}").map_err(|e| ServerError::io(&self.path, e))?;
        self.messages.push(message);
        Ok(())
    }

    pub fn messages(&self) -> &[ModelMessage] {
        &self.messages
    }
}
