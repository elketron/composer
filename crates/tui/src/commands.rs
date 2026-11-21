//! `:` commands.

use crate::action::Action;

/// Every command, as shown by help: (usage, what it does).
pub const COMMANDS: [(&str, &str); 14] = [
    (
        ":new [agent]",
        "start a new session (the default agent, or name one)",
    ),
    (":attach <id>", "switch to a session by id (or Space w)"),
    (
        ":model [id|none]",
        "the model this session uses (Tab lists the provider's); none: the server's again",
    ),
    (
        ":scope [name|none]",
        "narrow the session picker to a project",
    ),
    (
        ":mcp",
        "the MCP servers in config.toml: connects them and says which work, their tools and agents",
    ),
    (
        ":reload",
        "read config.toml again (the server also does when you save it); says what needs a restart",
    ),
    (
        ":file <path>|clear",
        "attach a file to the message (or paste or drop its path); clear removes them",
    ),
    (":open <url>", "open a page, e.g. :open memory/user"),
    (
        ":site [site]",
        "enter a site for this session (its page and tools); no name leaves it",
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
        "scope" => Ok(Action::Scope(arg)),
        "model" | "m" => Ok(match arg.as_deref() {
            None => Action::ShowModel,
            Some("none") => Action::SetModel(None),
            Some(model) => Action::SetModel(Some(model.to_string())),
        }),
        "mcp" => Ok(Action::McpServers),
        "reload" => Ok(Action::ReloadConfig),
        "file" => match arg.as_deref() {
            Some("clear") => Ok(Action::ClearAttachments),
            Some(path) => {
                let path = match path.strip_prefix("~/") {
                    Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
                    None => path.to_string(),
                };
                let path = std::path::absolute(&path)
                    .map(|p| p.display().to_string())
                    .unwrap_or(path);
                Ok(Action::AttachPath(path))
            }
            None => Err("usage: :file <path> (or :file clear)".into()),
        },
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
        "site" => Ok(Action::SetSite(arg.map(|s| {
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
            parse_command("new default"),
            Ok(Action::NewSession {
                agent: Some("default".into())
            })
        );
        assert_eq!(
            parse_command("open memory/user"),
            Ok(Action::OpenUrl("site://memory/user".into()))
        );
        assert_eq!(parse_command("mcp").unwrap(), Action::McpServers);
        assert_eq!(parse_command("reload").unwrap(), Action::ReloadConfig);
        assert_eq!(parse_command("scope").unwrap(), Action::Scope(None));
        assert_eq!(
            parse_command("site 3d").unwrap(),
            Action::SetSite(Some("3d".into()))
        );
        assert_eq!(
            parse_command("site site://3d/").unwrap(),
            Action::SetSite(Some("3d".into()))
        );
        assert_eq!(parse_command("site  ").unwrap(), Action::SetSite(None));
        assert!(parse_command("attach").is_err());
        assert!(parse_command("frobnicate").is_err());
    }
}
