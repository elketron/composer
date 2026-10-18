//! Drives the real app without a terminal, from a script, saving snapshots.
//!
//! ```text
//! size 140x40
//! wait connected
//! keys i what sites exist?<Esc><Enter>
//! wait turn 180          # until a turn finishes (timeout in seconds)
//! snap chat              # writes chat.html + chat.txt
//! keys <Space>p
//! wait text pages
//! ```
//!
//! Keys use vim notation: `<Enter> <Esc> <Space> <Tab> <BS> <Up> <Down> <C-w> <A-Enter> <lt>`.
//! Conditions: `connected`, `session`, `turn`, `idle`, `page`, `text <substring>`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::options::TuiOptions;
use crate::snapshot::{buffer_to_html, buffer_to_text};

/// Runs `script`; returns the HTML snapshots written to `out_dir`.
pub async fn run_script(
    options: TuiOptions,
    script: &str,
    out_dir: &Path,
) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir)?;
    let mut app = App::new(&options);
    let mut terminal = Terminal::new(TestBackend::new(140, 40))?;
    let mut turn_mark = 0;
    let mut snapshots = Vec::new();

    for (number, raw) in script.lines().enumerate() {
        let line = raw.split(" #").next().unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (command, arg) = line
            .split_once(' ')
            .map(|(c, a)| (c, a.trim()))
            .unwrap_or((line, ""));
        let context = || format!("line {}: {raw}", number + 1);
        match command {
            "size" => {
                let (w, h) = arg.split_once('x').context("size WxH")?;
                terminal = Terminal::new(TestBackend::new(w.parse()?, h.parse()?))?;
            }
            "keys" => {
                turn_mark = app.state.session.turns_done;
                for key in parse_keys(arg).with_context(context)? {
                    app.handle_key(key);
                    app.pump(Duration::from_millis(30)).await;
                }
            }
            "wait" => {
                let (condition, timeout) = split_timeout(arg);
                let deadline = tokio::time::Instant::now() + timeout;
                loop {
                    app.draw(&mut terminal)?;
                    if met(&app, &terminal, condition, turn_mark) {
                        break;
                    }
                    if tokio::time::Instant::now() > deadline {
                        let screen = buffer_to_text(terminal.backend().buffer());
                        bail!(
                            "{}: timed out waiting for `{condition}`\n{screen}",
                            context()
                        );
                    }
                    app.pump(Duration::from_millis(100)).await;
                }
            }
            "sleep" => {
                app.pump(Duration::from_millis(arg.parse().with_context(context)?))
                    .await
            }
            "snap" => {
                app.pump(Duration::from_millis(50)).await;
                app.draw(&mut terminal)?;
                let buffer = terminal.backend().buffer();
                let html = out_dir.join(format!("{arg}.html"));
                std::fs::write(&html, buffer_to_html(buffer, arg))?;
                std::fs::write(out_dir.join(format!("{arg}.txt")), buffer_to_text(buffer))?;
                snapshots.push(html);
            }
            other => bail!("{}: unknown command `{other}`", context()),
        }
    }
    Ok(snapshots)
}

fn split_timeout(arg: &str) -> (&str, Duration) {
    match arg.rsplit_once(' ') {
        Some((condition, secs)) if !arg.starts_with("text ") || secs.parse::<u64>().is_ok() => {
            match secs.parse() {
                Ok(secs) => (condition, Duration::from_secs(secs)),
                Err(_) => (arg, Duration::from_secs(60)),
            }
        }
        _ => (arg, Duration::from_secs(60)),
    }
}

fn met(app: &App, terminal: &Terminal<TestBackend>, condition: &str, turn_mark: u64) -> bool {
    let state = &app.state;
    match condition.split_once(' ').map_or(condition, |(c, _)| c) {
        "connected" => state.conn.is_connected(),
        "session" => state.session.current_info().is_some(),
        "turn" => state.session.turns_done > turn_mark,
        "idle" => state.session.current_info().is_some() && !state.session.is_busy(),
        "page" => state.page.current.is_some() && state.page.loading.is_none(),
        "text" => {
            let needle = condition.split_once(' ').map_or("", |(_, t)| t);
            buffer_to_text(terminal.backend().buffer()).contains(needle)
        }
        _ => false,
    }
}

/// Vim-style key notation → key events.
pub fn parse_keys(input: &str) -> anyhow::Result<Vec<KeyEvent>> {
    let mut keys = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '<' {
            let modifiers = if c.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            keys.push(KeyEvent::new(KeyCode::Char(c), modifiers));
            continue;
        }
        let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
        let (modifiers, base) = match name.split_once('-') {
            Some(("C", base)) => (KeyModifiers::CONTROL, base),
            Some(("A", base)) => (KeyModifiers::ALT, base),
            Some(("S", base)) => (KeyModifiers::SHIFT, base),
            _ => (KeyModifiers::NONE, name.as_str()),
        };
        let code = match base {
            "Enter" | "CR" => KeyCode::Enter,
            "Esc" => KeyCode::Esc,
            "Space" => KeyCode::Char(' '),
            "Tab" if modifiers == KeyModifiers::SHIFT => KeyCode::BackTab,
            "Tab" => KeyCode::Tab,
            "BS" => KeyCode::Backspace,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            "lt" => KeyCode::Char('<'),
            single if single.chars().count() == 1 => {
                KeyCode::Char(single.chars().next().unwrap_or(' '))
            }
            other => bail!("unknown key <{other}>"),
        };
        keys.push(KeyEvent::new(code, modifiers));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_notation() {
        let keys = parse_keys("iH<Enter><C-w>o<Space>p<lt>").unwrap();
        let codes: Vec<_> = keys.iter().map(|k| (k.code, k.modifiers)).collect();
        assert_eq!(
            codes,
            [
                (KeyCode::Char('i'), KeyModifiers::NONE),
                (KeyCode::Char('H'), KeyModifiers::SHIFT),
                (KeyCode::Enter, KeyModifiers::NONE),
                (KeyCode::Char('w'), KeyModifiers::CONTROL),
                (KeyCode::Char('o'), KeyModifiers::NONE),
                (KeyCode::Char(' '), KeyModifiers::NONE),
                (KeyCode::Char('p'), KeyModifiers::NONE),
                (KeyCode::Char('<'), KeyModifiers::NONE),
            ]
        );
    }
}
