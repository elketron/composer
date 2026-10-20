//! `:` commands.

use crate::action::Action;
use crate::store::types::PickerKind;

/// Every command, as shown by help: (usage, what it does).
pub const COMMANDS: [(&str, &str); 12] = [
    (
        ":new [agent]",
        "start a new session (default agent, or e.g. orchestrator)",
    ),
    (":attach <id>", "switch to a session by id (or Space w)"),
    (
        ":workers",
        "this orchestration's sessions (Space o; ]w / [w step through them)",
    ),
    (
        ":project [name|none]",
        "set the active project: a folder in the projects dir (or ~/path); `none` unsets it",
    ),
    (
        ":harness [on|off] [name]",
        "the project's external harnesses; turn Claude Code (or claude-code:<model>) on or off",
    ),
    (":open <url>", "open a page, e.g. :open memory/user"),
    (
        ":app [site]",
        "open a site's app in the browser (default: the page's site)",
    ),
    (":compact", "save memory, then summarize older messages now"),
    (":follow", "toggle following the agent's pages (F)"),
    (":back", "back to the previous session (ctrl-o)"),
    (":help", "this help (?)"),
    (":q", "quit"),
];

/// Parses a command line (without the `:`).
pub fn parse_command(line: &str) -> Result<Action, String> {
    let line = line.trim();
    let (name, arg) = line
        .split_once(char::is_whitespace)
        .map(|(n, a)| (n, a.trim()))
        .unwrap_or((line, ""));
    let arg = (!arg.is_empty()).then(|| arg.to_string());
    match name {
        "q" | "quit" => Ok(Action::Quit),
        "new" => Ok(Action::NewSession { agent: arg }),
        "attach" | "a" => arg
            .map(Action::AttachSession)
            .ok_or_else(|| "usage: :attach <session id>".into()),
        "workers" => Ok(Action::OpenPicker(PickerKind::Orchestration)),
        "project" | "p" => Ok(Action::SetProject(arg.filter(|a| a != "none"))),
        "harness" => {
            let words: Vec<&str> = arg.as_deref().unwrap_or("").split_whitespace().collect();
            let (on, rest) = match words.first() {
                Some(&"on") => (Some(true), &words[1..]),
                Some(&"off") => (Some(false), &words[1..]),
                _ => (None, &words[..]),
            };
            match rest {
                [] => Ok(Action::Harness {
                    on,
                    entry: "claude-code".into(),
                }),
                [entry] if on.is_some() => Ok(Action::Harness {
                    on,
                    entry: entry.to_string(),
                }),
                _ => Err("usage: :harness [on|off] [claude-code[:model]]".into()),
            }
        }
        "open" | "o" => arg
            .map(|url| {
                Action::OpenUrl(if url.starts_with("site://") {
                    url
                } else {
                    format!("site://{url}")
                })
            })
            .ok_or_else(|| "usage: :open <site://url>".into()),
        "follow" => Ok(Action::ToggleFollow),
        "compact" => Ok(Action::Compact),
        "app" => Ok(Action::OpenApp(arg.map(|s| {
            s.trim_start_matches("site://")
                .trim_end_matches('/')
                .to_string()
        }))),
        "back" => Ok(Action::JumpBack),
        "help" | "h" => Ok(Action::ToggleHelp),
        "" => Err(String::new()),
        other => Err(format!("unknown command `{other}` (:help lists them)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse_command("q"), Ok(Action::Quit));
        assert_eq!(
            parse_command("new orchestrator"),
            Ok(Action::NewSession {
                agent: Some("orchestrator".into())
            })
        );
        assert_eq!(
            parse_command("open memory/user"),
            Ok(Action::OpenUrl("site://memory/user".into()))
        );
        assert_eq!(parse_command("project"), Ok(Action::SetProject(None)));
        assert_eq!(parse_command("project none"), Ok(Action::SetProject(None)));
        assert_eq!(
            parse_command("harness off"),
            Ok(Action::Harness {
                on: Some(false),
                entry: "claude-code".into()
            })
        );
        assert_eq!(
            parse_command("harness on claude-code:opus"),
            Ok(Action::Harness {
                on: Some(true),
                entry: "claude-code:opus".into()
            })
        );
        assert!(matches!(
            parse_command("harness"),
            Ok(Action::Harness { on: None, .. })
        ));
        assert!(parse_command("harness sideways").is_err());
        assert!(parse_command("attach").is_err());
        assert!(parse_command("frobnicate").is_err());
    }
}
