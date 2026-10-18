use composer_protocol::ClientRequest;
use tokio::sync::mpsc;

use crate::action::Action;
use crate::api_client::ApiClient;
use crate::dispatcher::Dispatcher;
use crate::effect::Effect;
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
            Effect::EditPage(_)
            | Effect::ViewText { .. }
            | Effect::OpenBrowser(_)
            | Effect::OpenFile { .. }
            | Effect::Quit => {}
        }
    }
}
