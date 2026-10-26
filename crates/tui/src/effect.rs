//! Side effects returned by reducers and executed by the `EffectRunner` (or the app loop).

use composer_protocol::ClientRequest;

use crate::store::types::FileSource;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Send(ClientRequest),
    FetchPage(String),
    Search(String),
    LoadSites,
    /// `GET /api/projects?q=` → `ProjectsLoaded`.
    LoadProjects(String),
    /// Sites and every page, for completing `:` commands.
    LoadCatalog,
    /// Look up the site before opening its app (answered by `AppChecked`).
    CheckApp {
        site: String,
        url: String,
    },
    /// Fetch the active project's git changes for a session.
    FetchDiff(String),
    /// Suspend the TUI and open a project file in the editor at a line. Run by the app loop.
    OpenFile {
        path: String,
        line: u32,
    },
    /// Suspend the TUI and edit the page in nvim (temp file round trip). Run by the app loop.
    EditPage(String),
    /// Show text read-only in the editor (e.g. a recipe awaiting approval). Run by the app loop.
    ViewText {
        name: String,
        text: String,
    },
    /// Open a URL in the browser (an app). Run by the app loop.
    OpenBrowser(String),
    /// Upload a file to a session (answered by `FileUploaded`).
    UploadFile {
        id: u64,
        session: String,
        source: FileSource,
    },
    /// Read an image from the clipboard (answered by `ClipboardRead`).
    ReadClipboard,
    /// Put text on the clipboard (the app loop: it writes to the terminal too).
    CopyText(String),
    Quit,
}
