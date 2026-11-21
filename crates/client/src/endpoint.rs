use std::path::Path;

use serde_json::Value;

/// Where a client talks to: the server's base URL (`http://127.0.0.1:7420`) and its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    base: String,
    token: String,
}

/// The server's address when its config doesn't say.
const DEFAULT_BIND: &str = "127.0.0.1:7420";

impl Endpoint {
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            token: token.into(),
        }
    }

    /// The server this machine talks to for the home at `home`: a paired one (`composer login`
    /// wrote `remote.json`), else the local one (the home's `[server] bind`, its token). Reads
    /// only: the server makes the token when it first starts.
    pub fn discover(home: &Path) -> Result<Self, String> {
        if let Ok(text) = std::fs::read_to_string(home.join("remote.json")) {
            let remote: Value = serde_json::from_str(&text)
                .map_err(|e| format!("{}: {e}", home.join("remote.json").display()))?;
            if let (Some(server), Some(token)) =
                (remote["server"].as_str(), remote["token"].as_str())
            {
                return Ok(Self::new(server, token));
            }
        }
        let bind = std::fs::read_to_string(home.join("config.toml"))
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .and_then(|table| {
                table
                    .get("server")?
                    .get("bind")?
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_else(|| DEFAULT_BIND.to_string());
        let token_file = home.join("state/token");
        let token = std::fs::read_to_string(&token_file).map_err(|_| {
            format!(
                "no token in {} (has the server run once?)",
                token_file.display()
            )
        })?;
        Ok(Self::new(format!("http://{bind}"), token.trim()))
    }

    /// The server's base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Its WebSocket.
    pub fn ws_url(&self) -> String {
        format!(
            "{}/ws?token={}",
            self.base.replacen("http", "ws", 1),
            self.token
        )
    }

    /// One of its HTTP routes (`/api/sites`).
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// GET `path` as JSON, with the token.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let response = reqwest::Client::new()
            .get(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        json(response).await
    }

    /// POST `path` (no body) and its JSON answer, with the token.
    pub async fn post<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let response = reqwest::Client::new()
            .post(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        json(response).await
    }

    /// Uploads a file to a session's attachments; what a message then refers to.
    pub async fn upload(
        &self,
        session: &str,
        name: String,
        bytes: Vec<u8>,
    ) -> Result<composer_protocol::Attachment, String> {
        let part = reqwest::multipart::Part::bytes(bytes).file_name(name);
        let form = reqwest::multipart::Form::new().part("file", part);
        let response = reqwest::Client::new()
            .post(self.url(&format!("/api/sessions/{session}/attachments")))
            .bearer_auth(&self.token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let mut stored: Vec<composer_protocol::Attachment> = json(response).await?;
        stored
            .pop()
            .ok_or_else(|| "the server stored nothing".into())
    }
}

async fn json<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{status}: {text}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("{e}: {text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_else_local() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("state")).unwrap();
        std::fs::write(home.path().join("state/token"), "abc\n").unwrap();
        let local = Endpoint::discover(home.path()).unwrap();
        assert_eq!(local.base(), "http://127.0.0.1:7420");
        assert_eq!(local.ws_url(), "ws://127.0.0.1:7420/ws?token=abc");
        std::fs::write(
            home.path().join("config.toml"),
            "[server]\nbind = \"127.0.0.1:9000\"\n",
        )
        .unwrap();
        assert_eq!(
            Endpoint::discover(home.path()).unwrap().base(),
            "http://127.0.0.1:9000"
        );
        std::fs::write(
            home.path().join("remote.json"),
            r#"{"server": "https://box.ts.net", "token": "dev"}"#,
        )
        .unwrap();
        let paired = Endpoint::discover(home.path()).unwrap();
        assert_eq!(
            (paired.base(), paired.token()),
            ("https://box.ts.net", "dev")
        );
        assert!(Endpoint::discover(home.path().join("x").as_path()).is_err());
    }
}
