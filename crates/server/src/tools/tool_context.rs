use std::sync::Arc;

use composer_core::SiteUrl;
use tokio_util::sync::CancellationToken;

use crate::agent::AgentDefinition;
use crate::server::Server;
use crate::session::Session;

/// What a tool call runs against.
#[derive(Clone)]
pub struct ToolContext {
    pub server: Arc<Server>,
    pub session: Arc<Session>,
    pub agent: Arc<AgentDefinition>,
    /// Cancelled when the user cancels the turn.
    pub cancel: CancellationToken,
}

impl ToolContext {
    /// Parses a URL from the model. Forgiving: `memory/user` means `site://memory/user`.
    pub fn url(&self, raw: &str) -> Result<SiteUrl, String> {
        let raw = raw.trim();
        let full = if raw.starts_with("site://") {
            raw.to_string()
        } else {
            format!("site://{}", raw.trim_start_matches('/'))
        };
        SiteUrl::parse(&full).map_err(|e| e.to_string())
    }

    pub fn check_read(&self, url: &SiteUrl) -> Result<(), String> {
        if self.agent.sites.can_read(url.site()) {
            Ok(())
        } else {
            Err(format!(
                "agent `{}` may not read site `{}`",
                self.agent.name,
                url.site()
            ))
        }
    }

    pub fn check_write(&self, url: &SiteUrl) -> Result<(), String> {
        if self.agent.sites.can_write(url.site()) {
            Ok(())
        } else {
            Err(format!(
                "agent `{}` may not write to site `{}`",
                self.agent.name,
                url.site()
            ))
        }
    }
}
