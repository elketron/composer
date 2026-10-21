use composer_protocol::Attachment;
use composer_protocol::http::{
    PageResponse, ProjectDiff, ProjectSummary, PutPageRequest, PutPageResponse, SearchHit,
    SiteSummary,
};

use crate::store::types::PageView;

/// HTTP side of the server API.
#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl ApiClient {
    pub fn new(base: &str, token: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    pub async fn page(&self, url: &str) -> Result<PageView, String> {
        let response: PageResponse = self.get(&page_path(url)?).await?;
        Ok(PageView {
            url: response.url,
            title: response.title,
            frontmatter: response.frontmatter,
            frontmatter_error: response.frontmatter_error,
            raw: response.raw,
            backlinks: response.backlinks,
            hash: response.hash,
        })
    }

    /// Writes a page if it still has hash `if_match`.
    pub async fn put_page(
        &self,
        url: &str,
        content: String,
        if_match: Option<String>,
    ) -> Result<PutPageResponse, String> {
        let response = self
            .http
            .put(format!("{}{}", self.base, page_path(url)?))
            .bearer_auth(&self.token)
            .json(&PutPageRequest { content, if_match })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status() == reqwest::StatusCode::CONFLICT {
            return Err("the page changed on the server while you edited it".into());
        }
        Self::json(response).await
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchHit>, String> {
        let query: String = query.bytes().map(|b| format!("%{b:02X}")).collect();
        self.get(&format!("/api/search?limit=60&q={query}")).await
    }

    /// Every page (title and URL), for completion.
    pub async fn all_pages(&self) -> Result<Vec<SearchHit>, String> {
        self.get("/api/search?limit=5000&q=").await
    }

    pub async fn project_diff(&self, session: &str) -> Result<ProjectDiff, String> {
        self.get(&format!("/api/sessions/{session}/project/diff"))
            .await
    }

    pub async fn sites(&self) -> Result<Vec<SiteSummary>, String> {
        self.get("/api/sites").await
    }

    /// Projects matching `query`, the ones with something going on first.
    pub async fn projects(&self, query: &str) -> Result<Vec<ProjectSummary>, String> {
        self.get(&format!("/api/projects?q={}", urlencoding_minimal(query)))
            .await
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let response = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::json(response).await
    }

    /// Uploads one file to a session's attachments.
    pub async fn upload(
        &self,
        session: &str,
        name: String,
        bytes: Vec<u8>,
    ) -> Result<Attachment, String> {
        let part = reqwest::multipart::Part::bytes(bytes).file_name(name);
        let form = reqwest::multipart::Form::new().part("file", part);
        let response = self
            .http
            .post(format!("{}/api/sessions/{session}/attachments", self.base))
            .bearer_auth(&self.token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let mut stored: Vec<Attachment> = Self::json(response).await?;
        stored
            .pop()
            .ok_or_else(|| "the server stored nothing".into())
    }

    async fn json<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, String> {
        if !response.status().is_success() {
            let status = response.status();
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            return Err(body["error"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| status.to_string()));
        }
        response.json().await.map_err(|e| e.to_string())
    }
}

/// `site://memory/user.md` → `/api/pages/memory/user.md`
fn page_path(url: &str) -> Result<String, String> {
    let rest = url
        .strip_prefix("site://")
        .ok_or_else(|| format!("not a site url: {url}"))?;
    let rest = rest.split('#').next().unwrap_or(rest).trim_end_matches('/');
    Ok(format!("/api/pages/{rest}"))
}

/// Enough escaping for a search query in a URL.
fn urlencoding_minimal(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
