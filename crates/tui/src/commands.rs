//! `:` commands.

use crate::action::Action;
use crate::store::types::PickerKind;

/// Every command, as shown by help: (usage, what it does).
pub const COMMANDS: [(&str, &str); 20] = [
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
        ":project view [name]",
        "a project's works, sessions, branches and memory (gp; default: this session's)",
    ),
    (
        ":scope [name|none]",
        "narrow the session picker to a project (space P finds projects)",
    ),
    (
        ":pipeline [new <kind> [name]]",
        "the pipelines (sites of staged work); new makes one from a template (coding, video, research)",
    ),
    (
        ":work new <pipeline> <name> [project]",
        "start a piece of work in a pipeline (default project: the project view's or this session's)",
    ),
    (
        ":work run [work] [stage]",
        "run a work's next stage, or one again (default: the work of the open page; R in the project view)",
    ),
    (
        ":harness [on|off] [name]",
        "the project's external harnesses; turn Claude Code (or claude-code:<model>) on or off",
    ),
    (
        ":merge [ticket]",
        "merge a ticket's branch into the project (default: this worker's ticket; gd shows it)",
    ),
    (
        ":discard <ticket>",
        "delete a ticket's branch and worktree without merging",
    ),
    (
        ":file <path>|clear",
        "attach a file to the message (or paste its path while writing); clear removes them",
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
        "project" | "p" => match arg.as_deref() {
            Some("view") => Ok(Action::ProjectView(None)),
            Some(view) if view.starts_with("view ") => {
                Ok(Action::ProjectView(Some(view[5..].trim().to_string())))
            }
            _ => Ok(Action::SetProject(arg.filter(|a| a != "none"))),
        },
        "scope" => Ok(Action::Scope(arg)),
        "pipeline" | "pipelines" => {
            let words: Vec<&str> = arg.as_deref().unwrap_or("").split_whitespace().collect();
            match words[..] {
                [] => Ok(Action::Pipelines),
                ["new", kind] => Ok(Action::PipelineNew {
                    kind: kind.into(),
                    name: None,
                }),
                ["new", kind, name] => Ok(Action::PipelineNew {
                    kind: kind.into(),
                    name: Some(name.into()),
                }),
                _ => Err("usage: :pipeline [new <kind> [name]]".into()),
            }
        }
        "work" => {
            let words: Vec<&str> = arg.as_deref().unwrap_or("").split_whitespace().collect();
            match words[..] {
                ["new", pipeline, name] => Ok(Action::WorkNew {
                    pipeline: pipeline.into(),
                    name: name.into(),
                    project: None,
                }),
                ["new", pipeline, name, project] => Ok(Action::WorkNew {
                    pipeline: pipeline.into(),
                    name: name.into(),
                    project: Some(project.into()),
                }),
                ["run"] => Ok(Action::WorkRun {
                    work: None,
                    stage: None,
                }),
                ["run", work] => Ok(Action::WorkRun {
                    work: Some(work.into()),
                    stage: None,
                }),
                ["run", work, stage] => Ok(Action::WorkRun {
                    work: Some(work.into()),
                    stage: Some(stage.into()),
                }),
                _ => Err(
                    "usage: :work new <pipeline> <name> [project] · :work run [work] [stage]"
                        .into(),
                ),
            }
        }
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
        "merge" => Ok(Action::TicketMerge(arg)),
        "discard" => arg
            .map(Action::TicketDiscard)
            .ok_or_else(|| "usage: :discard <ticket> (deletes its branch and worktree)".into()),
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
        assert_eq!(parse_command("merge").unwrap(), Action::TicketMerge(None));
        assert_eq!(
            parse_command("discard 7").unwrap(),
            Action::TicketDiscard("7".into())
        );
        assert!(parse_command("discard").is_err());
        assert_eq!(parse_command("pipeline").unwrap(), Action::Pipelines);
        assert_eq!(
            parse_command("pipeline new video shorts").unwrap(),
            Action::PipelineNew {
                kind: "video".into(),
                name: Some("shorts".into())
            }
        );
        assert_eq!(
            parse_command("work new coding save-system").unwrap(),
            Action::WorkNew {
                pipeline: "coding".into(),
                name: "save-system".into(),
                project: None
            }
        );
        assert!(parse_command("work new coding").is_err());
        assert_eq!(
            parse_command("work run coding/save-system 02_plan").unwrap(),
            Action::WorkRun {
                work: Some("coding/save-system".into()),
                stage: Some("02_plan".into())
            }
        );
        assert_eq!(
            parse_command("project view").unwrap(),
            Action::ProjectView(None)
        );
        assert_eq!(
            parse_command("project view game").unwrap(),
            Action::ProjectView(Some("game".into()))
        );
        assert_eq!(
            parse_command("project game").unwrap(),
            Action::SetProject(Some("game".into()))
        );
        assert_eq!(parse_command("scope").unwrap(), Action::Scope(None));
        assert!(parse_command("attach").is_err());
        assert!(parse_command("frobnicate").is_err());
    }
}
