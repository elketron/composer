use crate::effect::Effect;
use crate::store::types::PageView;

/// The page viewer: current page, history, follow-agent.
#[derive(Debug)]
pub struct PageStore {
    pub current: Option<PageView>,
    pub loading: Option<String>,
    pub error: Option<String>,
    pub follow: bool,
    back: Vec<String>,
    forward: Vec<String>,
}

impl PageStore {
    pub fn new() -> Self {
        Self {
            current: None,
            loading: None,
            error: None,
            follow: true,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    /// Opens `url`, pushing the current page onto the back stack.
    pub fn navigate(&mut self, url: String) -> Effect {
        if let Some(current) = &self.current
            && current.url != url
        {
            self.back.push(current.url.clone());
            self.forward.clear();
        }
        self.load(url)
    }

    pub fn reload(&mut self) -> Option<Effect> {
        let url = self.current.as_ref()?.url.clone();
        Some(self.load(url))
    }

    pub fn back(&mut self) -> Option<Effect> {
        let url = self.back.pop()?;
        if let Some(current) = &self.current {
            self.forward.push(current.url.clone());
        }
        Some(self.load(url))
    }

    pub fn forward(&mut self) -> Option<Effect> {
        let url = self.forward.pop()?;
        if let Some(current) = &self.current {
            self.back.push(current.url.clone());
        }
        Some(self.load(url))
    }

    pub fn loaded(&mut self, page: PageView) {
        self.loading = None;
        self.error = None;
        self.current = Some(page);
    }

    pub fn failed(&mut self, url: &str, error: String) {
        if self.loading.as_deref() == Some(url) {
            self.loading = None;
        }
        self.error = Some(format!("{url}: {error}"));
    }

    /// Whether `url` is the page on screen (ignoring fragments).
    pub fn shows(&self, url: &str) -> bool {
        let strip = |u: &str| u.split('#').next().unwrap_or(u).to_string();
        self.current
            .as_ref()
            .is_some_and(|p| strip(&p.url) == strip(url))
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    fn load(&mut self, url: String) -> Effect {
        self.loading = Some(url.clone());
        Effect::FetchPage(url)
    }
}
