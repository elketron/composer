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

/// Opens `url` in the browser (`$BROWSER`, else `xdg-open`), without waiting for it. Choosing
/// the program is all that happens here; what to tell the user is [`browser_outcome`].
pub fn open_browser(url: &str) -> Vec<Action> {
    let opener = std::env::var("BROWSER").unwrap_or_else(|_| "xdg-open".into());
    let opened = Command::new(&opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string());
    browser_outcome(&opener, url, opened)
}

/// What to do once `opener` was asked for `url`, decided apart from the spawn so it can be
/// tested without a `$BROWSER` to race on. `$BROWSER` itself is honoured as it was, `wslview`
/// included; not being able to open is one case, whether the reason is WSL (the browser lives
/// on the Windows side), a bare tty, a container or ssh.
///
/// Opened: the one line it always was, and no clipboard write. Could not be opened: the URL is
/// the part worth having, so it goes out as a copy request (the app loop's existing
/// `Effect::CopyText` → `terminal::copy`: OSC 52 plus this machine's clipboard, which reaches
/// the clipboard of the machine you sit at, over WSL and ssh too) *and* is spelled out in the
/// message, for when even the clipboard is out of reach. The message comes last so it is what
/// the status bar keeps rather than the copy's own "copied" flash.
pub fn browser_outcome(opener: &str, url: &str, opened: Result<(), String>) -> Vec<Action> {
    match opened {
        Ok(()) => vec![Action::Flash(
            "app opened in the browser".into(),
            FlashLevel::Info,
        )],
        Err(error) => vec![
            Action::CopyText(url.to_string()),
            Action::Flash(
                format!(
                    "no browser here ({opener}: {error}) — copied to your clipboard, \
                     paste into your browser: {url}"
                ),
                FlashLevel::Error,
            ),
        ],
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

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:7420/w/default/a/app_x9tr/?session=s7";

    /// The one message the actions leave on the status bar.
    fn message(actions: &[Action]) -> (String, FlashLevel) {
        let mut found = actions.iter().filter_map(|action| match action {
            Action::Flash(text, level) => Some((text.clone(), *level)),
            _ => None,
        });
        let one = found.next().expect("a message");
        assert!(found.next().is_none(), "exactly one message");
        one
    }

    #[test]
    fn an_opener_that_cannot_run_shows_the_url_and_copies_it() {
        let actions = browser_outcome(
            "xdg-open",
            URL,
            Err("No such file or directory (os error 2)".into()),
        );
        assert!(
            actions.contains(&Action::CopyText(URL.into())),
            "the URL is asked of the clipboard (OSC 52): {actions:?}"
        );
        let (text, level) = message(&actions);
        assert_eq!(level, FlashLevel::Error);
        assert!(text.contains(URL), "the URL is readable: {text}");
        assert!(text.contains("xdg-open"), "names what did not run: {text}");
        assert!(text.contains("No such file"), "and why: {text}");
        assert!(text.contains("clipboard"), "and the next step: {text}");
        let copied = actions
            .iter()
            .position(|a| matches!(a, Action::CopyText(_)))
            .expect("a copy");
        let flashed = actions
            .iter()
            .position(|a| matches!(a, Action::Flash(..)))
            .expect("a message");
        assert!(
            copied < flashed,
            "the message is the last word, not the copy's own flash: {actions:?}"
        );
    }

    #[test]
    fn an_opener_that_worked_says_only_that() {
        assert_eq!(
            browser_outcome("wslview", URL, Ok(())),
            [Action::Flash(
                "app opened in the browser".into(),
                FlashLevel::Info
            )],
            "the success path is unchanged: one line, no copy"
        );
    }

    #[test]
    fn the_browser_env_names_the_opener_that_was_tried() {
        // The only test that touches $BROWSER: the environment is process-wide.
        let before = std::env::var("BROWSER").ok();
        unsafe { std::env::set_var("BROWSER", "composer-no-such-browser-0207") };
        let actions = open_browser(URL);
        match before {
            Some(value) => unsafe { std::env::set_var("BROWSER", value) },
            None => unsafe { std::env::remove_var("BROWSER") },
        }
        let (text, level) = message(&actions);
        assert_eq!(level, FlashLevel::Error);
        assert!(
            text.contains("composer-no-such-browser-0207"),
            "it says which program it tried: {text}"
        );
        assert!(text.contains(URL), "and gives the URL: {text}");
        assert!(
            actions.contains(&Action::CopyText(URL.into())),
            "and copies it: {actions:?}"
        );
    }
}
