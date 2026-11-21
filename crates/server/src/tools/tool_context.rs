use std::sync::Arc;

use composer_core::SiteUrl;
use tokio_util::sync::CancellationToken;

use crate::agent::{AgentDefinition, Site};
use crate::server::Server;
use crate::session::Session;

/// What a tool call runs against.
#[derive(Clone)]
pub struct ToolContext {
    pub server: Arc<Server>,
    pub session: Arc<Session>,
    pub agent: Arc<AgentDefinition>,
    /// The site the agent entered, if any: its tools are available too.
    pub site: Option<Arc<Site>>,
    /// Cancelled when the user cancels the turn.
    pub cancel: CancellationToken,
}

impl ToolContext {
    /// A context for running tools in `session` from outside a turn (the MCP bridge's
    /// `tool.call`): its agent and site as they are now, never cancelled.
    pub fn for_session(server: &Arc<Server>, session: &str) -> Result<Self, String> {
        let session = server.sessions.get(session).map_err(|e| e.to_string())?;
        let info = session.info();
        let agent = AgentDefinition::load(&server.web, &info.agent).map_err(|e| e.to_string())?;
        let site = info
            .site
            .as_deref()
            .and_then(|name| Site::load(&server.web, name).ok())
            .map(Arc::new);
        Ok(Self {
            server: server.clone(),
            session,
            agent: Arc::new(agent),
            site,
            cancel: CancellationToken::new(),
        })
    }

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
