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

/// Opens `text` read-only in `$EDITOR` (nvim gets `-R`). Nothing is saved.
pub fn view_text(name: &str, text: &str) -> Action {
    let result = (|| -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join(name);
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".into());
        let mut command = Command::new(&editor);
        if editor.ends_with("vim") {
            command.arg("-R");
        }
        command
            .arg(&path)
            .status()
            .map_err(|e| format!("{editor}: {e}"))?;
        Ok(())
    })();
    match result {
        Ok(()) => Action::Flash("y approves · n denies".into(), FlashLevel::Info),
        Err(error) => Action::Flash(error, FlashLevel::Error),
    }
}

/// Opens `url` in the browser (`$BROWSER`, else `xdg-open`), without waiting for it.
pub fn open_browser(url: &str) -> Action {
    let opener = std::env::var("BROWSER").unwrap_or_else(|_| "xdg-open".into());
    let spawned = Command::new(&opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match spawned {
        Ok(_) => Action::Flash("app opened in the browser".into(), FlashLevel::Info),
        Err(e) => Action::Flash(format!("{opener}: {e}"), FlashLevel::Error),
    }
}

/// Opens a project file in `$EDITOR` at `line` (`+line` works for vi, vim, nvim, helix, ...).
pub fn open_file(path: &str, line: u32) -> Action {
    if !std::path::Path::new(path).is_file() {
        return Action::Flash(format!("{path} is not on this machine"), FlashLevel::Warn);
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".into());
    match Command::new(&editor)
        .arg(format!("+{line}"))
        .arg(path)
        .status()
    {
        Ok(_) => Action::RefreshChanges,
        Err(e) => Action::Flash(format!("{editor}: {e}"), FlashLevel::Error),
    }
}
