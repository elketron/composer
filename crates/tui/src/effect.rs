//! Side effects returned by reducers and executed by the `EffectRunner` (or the app loop).

use composer_protocol::ClientRequest;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Send(ClientRequest),
    FetchPage(String),
    Search(String),
    LoadSites,
    /// Suspend the TUI and edit the page in nvim (temp file round trip). Run by the app loop.
    EditPage(String),
    Quit,
}
