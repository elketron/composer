/// Which kind of credential authenticated a request. The one place that tells the machine's own
/// user from everyone else; routes ask it what the caller may do instead of re-deriving it.
///
/// `require_token` (auth.rs) puts it in the request's extensions. When a request presents
/// several valid credentials (a header and a cookie), the first of `Authorization`, `?token=`,
/// `?once=`, the cookie decides. A browser session (the cookie, see
/// [`crate::api::WebSessions`]) is the kind of the credential it was made from: a session a
/// device opened is that device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// The workspace's own token (`state/token`): what the CLI, the TUI and the desktop on this
    /// machine read from disk.
    Local,
    /// A token of a device paired to the workspace (a phone, a browser): which one, by its name
    /// and by its token's hash (names are reused after a revoke, hashes are not).
    Device { name: String, token_hash: String },
}

impl Credential {
    /// Whether the caller may operate the application through the live view (Drive): only the
    /// machine's own user. Paired devices watch and annotate.
    pub fn may_drive(&self) -> bool {
        *self == Self::Local
    }
}
