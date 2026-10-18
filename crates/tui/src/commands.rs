//! `:` commands.

use crate::action::Action;

/// Every command, as shown by help: (usage, what it does).
pub const COMMANDS: [(&str, &str); 10] = [
    (
        ":new [agent]",
        "start a new session (default agent, or e.g. orchestrator)",
    ),
    (":attach <id>", "switch to a session by id (or Space w)"),
    (
        ":project [path]",
        "set the active project; without a path, clear it",
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
        "project" | "p" => Ok(Action::SetProject(arg)),
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
        assert!(parse_command("attach").is_err());
        assert!(parse_command("frobnicate").is_err());
    }
}
