use std::path::Path;

use serde_json::Value;

/// Where a client talks to: a workspace's base URL (`http://127.0.0.1:7420/w/default`) and its
/// token.
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

    /// The server this machine talks to for `workspace` of the home at `home`: a paired one
    /// (`composer login` wrote `remote.json`), else the local one (the home's `[server] bind`, the
    /// workspace's token). Reads only: the server makes the token when it first starts.
    pub fn discover(home: &Path, workspace: &str) -> Result<Self, String> {
        if let Ok(text) = std::fs::read_to_string(home.join("remote.json")) {
            let remote: Value = serde_json::from_str(&text)
                .map_err(|e| format!("{}: {e}", home.join("remote.json").display()))?;
            if let (Some(server), Some(token)) =
                (remote["server"].as_str(), remote["token"].as_str())
            {
                return Ok(Self::new(server, token));
            }
        }
        let workspace_root = home.join("workspaces").join(workspace);
        let bind = [
            home.join("global/config.toml"),
            workspace_root.join("config.toml"),
        ]
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .filter_map(|text| text.parse::<toml::Table>().ok())
        .filter_map(|table| {
            table
                .get("server")?
                .get("bind")?
                .as_str()
                .map(str::to_string)
        })
        .next_back()
        .unwrap_or_else(|| DEFAULT_BIND.to_string());
        let token_file = workspace_root.join("state/token");
        let token = std::fs::read_to_string(&token_file).map_err(|_| {
            format!(
                "no token in {} (is the workspace `{workspace}` there, and has the server run once?)",
                token_file.display()
            )
        })?;
        Ok(Self::new(
            format!("http://{bind}/w/{workspace}"),
            token.trim(),
        ))
    }

    /// The workspace's base URL.
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

    /// One of its HTTP routes (`/api/apps`).
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
        let ws = home.path().join("workspaces/default");
        std::fs::create_dir_all(ws.join("state")).unwrap();
        std::fs::create_dir_all(home.path().join("global")).unwrap();
        std::fs::write(ws.join("state/token"), "abc\n").unwrap();
        let local = Endpoint::discover(home.path(), "default").unwrap();
        assert_eq!(local.base(), "http://127.0.0.1:7420/w/default");
        assert_eq!(local.ws_url(), "ws://127.0.0.1:7420/w/default/ws?token=abc");
        std::fs::write(
            home.path().join("global/config.toml"),
            "[server]\nbind = \"127.0.0.1:9000\"\n",
        )
        .unwrap();
        assert_eq!(
            Endpoint::discover(home.path(), "default").unwrap().base(),
            "http://127.0.0.1:9000/w/default"
        );
        std::fs::write(
            home.path().join("remote.json"),
            r#"{"server": "https://box.ts.net/w/default", "token": "dev"}"#,
        )
        .unwrap();
        let paired = Endpoint::discover(home.path(), "default").unwrap();
        assert_eq!(
            (paired.base(), paired.token()),
            ("https://box.ts.net/w/default", "dev")
        );
        assert!(Endpoint::discover(home.path().join("x").as_path(), "b").is_err());
    }
}
