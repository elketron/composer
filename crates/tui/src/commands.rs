//! `:` commands.

use composer_protocol::RunKind;

use crate::action::Action;

/// The run kinds by the names commands take, first the default.
pub const RUN_KINDS: [(&str, RunKind); 3] = [
    ("agentic", RunKind::Agentic),
    ("workflow", RunKind::Workflow),
    ("single", RunKind::SingleShot),
];

/// A run kind by its name (`single-shot` and `single_shot` too).
pub fn parse_kind(name: &str) -> Option<RunKind> {
    match name.trim().to_lowercase().as_str() {
        "single-shot" | "single_shot" => Some(RunKind::SingleShot),
        name => RUN_KINDS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k),
    }
}

/// A run kind's name, as commands take it and lists show it.
pub fn kind_name(kind: RunKind) -> &'static str {
    RUN_KINDS
        .iter()
        .find(|(_, k)| *k == kind)
        .map(|(n, _)| *n)
        .expect("every kind has a name")
}

/// A session's kind as lists show it, with the outside agent that runs it: `agentic · claude`.
pub fn session_label(session: &composer_protocol::SessionInfo) -> String {
    match &session.harness {
        Some(harness) => format!("{} · {harness}", kind_name(session.kind)),
        None => kind_name(session.kind).to_string(),
    }
}

/// Every command, as shown by help: (usage, what it does).
pub const COMMANDS: [(&str, &str); 19] = [
    (
        ":new [agentic|workflow|single]",
        "start a new session: agentic (the default), a read-only workflow run, or a single shot",
    ),
    (
        ":new agentic --harness <name>",
        "an agentic session run by an outside agent (an [acp.<name>] of config.toml)",
    ),
    (":attach <id>", "switch to a session by id (or Space w)"),
    (
        ":delete [id]",
        "delete a session (this one unless named): its chat, terminals and folder; asks first",
    ),
    (
        ":model [id|none]",
        "the model this session uses (Tab lists the provider's); none: the server's again",
    ),
    (
        ":scope [name|none]",
        "narrow the session picker to the sessions in a site",
    ),
    (
        ":mcp",
        "the MCP servers in config.toml: connects them and says which work, their tools and sites",
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
        "set the site of this session (set_site: its page, tools and folder); no name leaves it",
    ),
    (
        ":term [command]",
        "a terminal on the stage: a shell, or a program (:term nvim notes.md); ctrl-space leaves it",
    ),
    (
        ":ps [id]",
        "the session's background processes (the agent's process_start); an id opens its terminal",
    ),
    (
        ":stop [id]",
        "stop a background process: the one shown, or the only one, unless named",
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
        "new" => parse_new(arg.as_deref().unwrap_or("")),
        "delete" => Ok(Action::DeleteSession(arg)),
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
        "term" | "terminal" => Ok(Action::OpenTerminal(arg)),
        "ps" => Ok(match arg {
            Some(id) => Action::ShowProcess(id),
            None => Action::ListProcesses,
        }),
        "stop" => Ok(Action::StopProcess(arg)),
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

/// `:new [kind] [--harness <name>]`: a harness runs agentic sessions only (and is one).
fn parse_new(args: &str) -> Result<Action, String> {
    let mut kind = None;
    let mut harness = None;
    let mut words = args.split_whitespace();
    while let Some(word) = words.next() {
        match word {
            "--harness" => match words.next() {
                Some(name) => harness = Some(name.to_string()),
                None => return Err("usage: :new agentic --harness <name>".into()),
            },
            name if kind.is_none() => {
                kind = Some(parse_kind(name).ok_or_else(|| {
                    format!("`{name}` isn't a run kind: agentic, workflow or single")
                })?)
            }
            other => {
                return Err(format!(
                    "unexpected `{other}`: :new [kind] [--harness <name>]"
                ));
            }
        }
    }
    if harness.is_some() {
        match kind {
            None => kind = Some(RunKind::Agentic),
            Some(RunKind::Agentic) => {}
            Some(other) => {
                return Err(format!(
                    "a harness runs agentic sessions, not {}",
                    kind_name(other)
                ));
            }
        }
    }
    Ok(Action::NewSession { kind, harness })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse_command("q"), Ok(Action::Quit));
        assert_eq!(
            parse_command("new workflow"),
            Ok(Action::NewSession {
                kind: Some(RunKind::Workflow),
                harness: None,
            })
        );
        assert_eq!(
            parse_command("new"),
            Ok(Action::NewSession {
                kind: None,
                harness: None
            })
        );
        assert!(parse_command("new default").is_err());
        assert_eq!(
            parse_command("new --harness claude"),
            Ok(Action::NewSession {
                kind: Some(RunKind::Agentic),
                harness: Some("claude".into()),
            })
        );
        assert_eq!(
            parse_command("new agentic --harness claude"),
            parse_command("new --harness claude")
        );
        assert!(parse_command("new workflow --harness claude").is_err());
        assert!(parse_command("new agentic --harness").is_err());
        assert_eq!(
            parse_command("delete").unwrap(),
            Action::DeleteSession(None)
        );
        assert_eq!(
            parse_command("delete s1").unwrap(),
            Action::DeleteSession(Some("s1".into()))
        );
        assert_eq!(
            parse_command("ps t2").unwrap(),
            Action::ShowProcess("t2".into())
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
        assert_eq!(parse_command("term").unwrap(), Action::OpenTerminal(None));
        assert_eq!(
            parse_command("term nvim notes.md").unwrap(),
            Action::OpenTerminal(Some("nvim notes.md".into()))
        );
        assert_eq!(parse_command("ps").unwrap(), Action::ListProcesses);
        assert_eq!(
            parse_command("stop t1").unwrap(),
            Action::StopProcess(Some("t1".into()))
        );
        assert!(parse_command("attach").is_err());
        assert!(parse_command("frobnicate").is_err());
    }
}
