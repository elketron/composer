//! Editing a page in nvim: the temp file round trip (same for local and remote servers).

use std::process::Command;

use crate::action::Action;
use crate::api_client::ApiClient;
use crate::store::types::FlashLevel;

/// Fetches the page, opens it in `$EDITOR` (default nvim) and writes it back if it
/// changed, refusing if the page changed on the server meanwhile. The terminal must
/// already be restored to normal mode.
pub async fn edit_page(api: &ApiClient, url: &str) -> Action {
    match round_trip(api, url).await {
        Ok(Some(saved)) => Action::Flash(format!("saved {saved}"), FlashLevel::Info),
        Ok(None) => Action::Flash("no changes".into(), FlashLevel::Info),
        Err(error) => Action::Flash(error, FlashLevel::Error),
    }
}

async fn round_trip(api: &ApiClient, url: &str) -> Result<Option<String>, String> {
    let page = api.page(url).await?;
    let name = page.url.rsplit('/').next().unwrap_or("page.md").to_string();
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join(&name);
    std::fs::write(&path, &page.raw).map_err(|e| e.to_string())?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".into());
    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| format!("{editor}: {e}"))?;
    if !status.success() {
        return Err(format!("{editor} exited with {status}"));
    }
    let edited = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if edited == page.raw {
        return Ok(None);
    }
    let saved = api.put_page(&page.url, edited, Some(page.hash)).await?;
    Ok(Some(saved.url))
}
