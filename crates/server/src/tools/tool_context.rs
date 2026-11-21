use std::sync::Arc;

use composer_core::SiteUrl;
use composer_protocol::RunKind;
use tokio_util::sync::CancellationToken;

use crate::agent::Site;
use crate::server::Server;
use crate::session::{RunSink, Session};

/// What a tool call runs against.
#[derive(Clone)]
pub struct ToolContext {
    pub server: Arc<Server>,
    /// Where the run puts its events and keeps its site.
    pub run: Arc<dyn RunSink>,
    /// The run's kind: which tools it has.
    pub kind: RunKind,
    /// Narrows the kind's tools to these (the memory pass of a compaction).
    pub only: Option<&'static [&'static str]>,
    /// The site the agent entered, if any: its tools are available too.
    pub site: Option<Arc<Site>>,
    /// Cancelled when the user cancels the turn.
    pub cancel: CancellationToken,
}

impl ToolContext {
    /// A context for running tools in `session` from outside a turn (the MCP bridge's
    /// `tool.call`): its kind and site as they are now, never cancelled.
    pub fn for_session(server: &Arc<Server>, session: &str) -> Result<Self, String> {
        let session = server.sessions.get(session).map_err(|e| e.to_string())?;
        let info = session.info();
        let site = info
            .site
            .as_deref()
            .and_then(|name| Site::load(&server.web, name).ok())
            .map(Arc::new);
        Ok(Self {
            server: server.clone(),
            run: server.sink(&session),
            kind: info.kind,
            only: None,
            site,
            cancel: CancellationToken::new(),
        })
    }

    /// The stored session the run belongs to; an error for a tool that needs one (a desk,
    /// terminals, approvals) in a run that is not stored.
    pub fn session(&self) -> Result<Arc<Session>, String> {
        self.run
            .session()
            .ok_or_else(|| "this tool needs a stored session; this run is not stored".to_string())
    }

    /// The stored session to ask the user's approval of `what` in. A run that is not stored has
    /// nobody to ask: it is stopped with a clear error instead of waiting for an answer that
    /// cannot come.
    pub fn approval_session(&self, what: &str) -> Result<Arc<Session>, String> {
        self.run.session().ok_or_else(|| {
            let error = format!(
                "{what} needs the user's approval, and this run is not stored, so nobody can \
                 give it: the run stops. Allow it in config.toml, or store the run."
            );
            self.run.abort(error.clone());
            error
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
}
