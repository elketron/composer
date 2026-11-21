use std::sync::Arc;

use composer_core::Web;
use composer_protocol::RunKind;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agent::{RunSettings, Site, folder_instructions};
use crate::server::Server;
use crate::session::run_once;

/// A run to start: its kind, the site and workflow folder it is in, and its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    pub kind: RunKind,
    pub site: Option<String>,
    pub folder: Option<String>,
    pub input: String,
}

impl RunRequest {
    /// The request with its site's own name, after checking the site exists and the folder is
    /// inside it with instructions (`CONTEXT.md` or `index.md`).
    pub fn checked(mut self, web: &Web) -> Result<Self, String> {
        self.site = match self.site.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(name) => Some(Site::load(web, name)?.name),
        };
        self.folder = self
            .folder
            .map(|f| f.trim().trim_matches('/').to_string())
            .filter(|f| !f.is_empty());
        if self.folder.is_some() {
            folder_instructions(web, self.site.as_deref(), self.folder.as_deref())?;
        }
        Ok(self)
    }

    /// How it runs: its kind's settings, following its folder.
    pub fn settings(&self) -> RunSettings {
        RunSettings {
            folder: self.folder.clone(),
            ..RunSettings::new(self.kind)
        }
    }

    /// Runs it without storing anything, and gives its result: a single shot's answer, a
    /// workflow run's `finish` value, an agentic run's last answer.
    pub async fn run_unstored(
        &self,
        server: &Arc<Server>,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        run_once(
            server,
            &self.settings(),
            &self.input,
            self.site.clone(),
            cancel,
        )
        .await
        .0
    }
}
