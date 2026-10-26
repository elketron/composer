use composer_protocol::ClientRequest;
use tokio::sync::mpsc;

use crate::action::Action;
use crate::api_client::ApiClient;
use crate::dispatcher::Dispatcher;
use crate::effect::Effect;
use crate::store::types::FileSource;
use crate::ws_link;

/// Executes effects off the UI thread; results come back as actions.
pub struct EffectRunner {
    api: ApiClient,
    ws: mpsc::UnboundedSender<ClientRequest>,
    dispatcher: Dispatcher,
}

impl EffectRunner {
    pub fn new(server_url: &str, token: &str, dispatcher: Dispatcher) -> Self {
        Self {
            api: ApiClient::new(server_url, token),
            ws: ws_link::spawn(server_url, token, dispatcher.clone()),
            dispatcher,
        }
    }

    pub fn api(&self) -> &ApiClient {
        &self.api
    }

    /// Runs an effect. `EditPage`, `ViewText` and `Quit` belong to the app loop and are ignored here.
    pub fn run(&self, effect: Effect) {
        let api = self.api.clone();
        let dispatcher = self.dispatcher.clone();
        match effect {
            Effect::Send(request) => {
                let _ = self.ws.send(request);
            }
            Effect::FetchPage(url) => {
                tokio::spawn(async move {
                    dispatcher.dispatch(match api.page(&url).await {
                        Ok(page) => Action::PageLoaded(page),
                        Err(error) => Action::PageFailed { url, error },
                    });
                });
            }
            Effect::Search(query) => {
                tokio::spawn(async move {
                    let hits = api.search(&query).await.unwrap_or_default();
                    dispatcher.dispatch(Action::SearchResults { query, hits });
                });
            }
            Effect::FetchDiff(session) => {
                tokio::spawn(async move {
                    dispatcher.dispatch(match api.project_diff(&session).await {
                        Ok(diff) => Action::DiffLoaded(diff),
                        Err(error) => Action::DiffFailed(error),
                    });
                });
            }
            Effect::LoadProjects(query) => {
                tokio::spawn(async move {
                    let projects = api.projects(&query).await.unwrap_or_default();
                    dispatcher.dispatch(Action::ProjectsLoaded { query, projects });
                });
            }
            Effect::LoadSites => {
                tokio::spawn(async move {
                    dispatcher.dispatch(Action::SitesLoaded(api.sites().await.unwrap_or_default()));
                });
            }
            Effect::LoadCatalog => {
                tokio::spawn(async move {
                    let (sites, pages) = tokio::join!(api.sites(), api.all_pages());
                    dispatcher.dispatch(Action::CatalogLoaded {
                        sites: sites.unwrap_or_default(),
                        pages: pages.unwrap_or_default(),
                    });
                });
            }
            Effect::CheckApp { site, url } => {
                tokio::spawn(async move {
                    let sites = api.sites().await.ok();
                    dispatcher.dispatch(Action::AppChecked { site, url, sites });
                });
            }
            Effect::UploadFile {
                id,
                session,
                source,
            } => {
                tokio::spawn(async move {
                    let result = match read_source(source).await {
                        Ok((name, bytes)) => api.upload(&session, name, bytes).await,
                        Err(e) => Err(e),
                    };
                    dispatcher.dispatch(Action::FileUploaded { id, result });
                });
            }
            Effect::ReadClipboard => {
                tokio::spawn(async move {
                    let read = tokio::task::spawn_blocking(clipboard_png)
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                    dispatcher.dispatch(Action::ClipboardRead(read));
                });
            }
            Effect::EditPage(_)
            | Effect::CopyText(_)
            | Effect::ViewText { .. }
            | Effect::OpenBrowser(_)
            | Effect::OpenFile { .. }
            | Effect::Quit => {}
        }
    }
}

/// The file's name and bytes.
async fn read_source(source: FileSource) -> Result<(String, Vec<u8>), String> {
    match source {
        FileSource::Bytes { name, bytes } => Ok((name, bytes)),
        FileSource::Path(path) => {
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".into());
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|e| format!("{path}: {e}"))?;
            Ok((name, bytes))
        }
    }
}

/// The clipboard's image as PNG.
fn clipboard_png() -> Result<(String, Vec<u8>), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let image = clipboard
        .get_image()
        .map_err(|_| "no image on the clipboard".to_string())?;
    let rgba = image::RgbaImage::from_raw(
        image.width as u32,
        image.height as u32,
        image.bytes.into_owned(),
    )
    .ok_or("the clipboard image is malformed")?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let name = format!("clipboard-{}.png", unix_stamp());
    Ok((name, png))
}

/// Seconds since the epoch, for clipboard image names.
fn unix_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    secs.to_string()
}
