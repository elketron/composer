//! Completing `:` commands with Tab, as vim's command line does: command names, then each
//! command's argument (session ids, agents, sites and pages, project directories).

use crate::commands::COMMANDS;
use crate::store::AppState;

/// One choice in the menu.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// What replaces the word being completed.
    pub text: String,
    /// Shown dimmed beside it (a session's preview, a page's title, a command's help).
    pub detail: String,
}

/// The candidates for the word that ends the line, which starts at byte `start`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Completions {
    pub start: usize,
    pub items: Vec<Candidate>,
}

/// Tab cycling through completions, as the command line remembers it.
#[derive(Debug, Clone, PartialEq)]
pub struct Cycle {
    /// The line as typed before the first Tab (shown again after the last candidate).
    pub original: String,
    pub start: usize,
    pub items: Vec<Candidate>,
    /// The candidate in the line; `None` while showing what was typed.
    pub selected: Option<usize>,
}

/// Most candidates the menu lists.
const LIMIT: usize = 200;

/// Completions for `line` (the command line without its `:`).
pub fn complete(line: &str, state: &AppState) -> Completions {
    let Some(space) = line.find(char::is_whitespace) else {
        return Completions {
            start: 0,
            items: command_names(line),
        };
    };
    let name = &line[..space];
    let start = space + line[space..].len() - line[space..].trim_start().len();
    let arg = &line[start..];
    let items = match name {
        "attach" | "a" => sessions(arg, state),
        "new" => agents(arg, state),
        "open" | "o" => pages(arg, state),
        "app" => app_sites(arg, state),
        "work" => subcommands(
            arg,
            &[
                (
                    "new",
                    "a piece of work in a pipeline: new <pipeline> <name> [project]",
                ),
                ("run", "its next stage, or one again: run [work] [stage]"),
            ],
        ),
        "pipeline" | "pipelines" => subcommands(
            arg,
            &[("new", "a pipeline from a template: new <kind> [name]")],
        ),
        "project" | "p" => {
            let view = subcommands(arg, &[("view", "its works, sessions, branches and memory")]);
            let unset = state
                .session
                .current_info()
                .and_then(|s| s.project.as_deref())
                .filter(|_| starts_with("none", arg))
                .map(|project| Candidate {
                    text: "none".into(),
                    detail: format!("unset {project}"),
                });
            let folders = if is_local(&state.conn.server_url) {
                projects(arg, state.projects_dir.as_deref())
            } else {
                Vec::new()
            };
            view.into_iter().chain(unset).chain(folders).collect()
        }
        _ => Vec::new(),
    };
    Completions {
        start,
        items: items.into_iter().take(LIMIT).collect(),
    }
}

fn starts_with(text: &str, prefix: &str) -> bool {
    text.to_lowercase().starts_with(&prefix.to_lowercase())
}

/// One candidate per command. A command with several forms (`:project`, `:project view`)
/// lists them as its detail.
fn command_names(prefix: &str) -> Vec<Candidate> {
    let mut names: Vec<(&str, Vec<(&str, &str)>)> = Vec::new();
    for (usage, help) in COMMANDS {
        let usage = usage.trim_start_matches(':');
        let Some(name) = usage.split_whitespace().next() else {
            continue;
        };
        let form = usage[name.len()..].trim();
        match names.iter_mut().find(|(n, _)| *n == name) {
            Some((_, forms)) => forms.push((form, help)),
            None => names.push((name, vec![(form, help)])),
        }
    }
    names
        .into_iter()
        .filter(|(name, _)| starts_with(name, prefix))
        .map(|(name, forms)| Candidate {
            text: name.to_string(),
            detail: match forms.as_slice() {
                [(_, help)] => (*help).to_string(),
                forms => forms
                    .iter()
                    .map(|(form, _)| *form)
                    .collect::<Vec<_>>()
                    .join(" · "),
            },
        })
        .collect()
}

/// A command's subcommands, as its first argument.
fn subcommands(arg: &str, choices: &[(&str, &str)]) -> Vec<Candidate> {
    if arg.contains(char::is_whitespace) {
        return Vec::new();
    }
    choices
        .iter()
        .filter(|(name, _)| starts_with(name, arg))
        .map(|(name, detail)| Candidate {
            text: name.to_string(),
            detail: detail.to_string(),
        })
        .collect()
}

/// By id; when no id starts with it, by words in the agent or the last message. Newest first.
fn sessions(arg: &str, state: &AppState) -> Vec<Candidate> {
    let all = &state.session.sessions;
    let needle = arg.to_lowercase();
    let by_id: Vec<_> = all.iter().filter(|s| starts_with(&s.id, arg)).collect();
    let matches = if by_id.is_empty() {
        all.iter()
            .filter(|s| {
                let preview = s.preview.as_deref().unwrap_or("");
                s.agent.to_lowercase().contains(&needle) || preview.to_lowercase().contains(&needle)
            })
            .collect()
    } else {
        by_id
    };
    matches
        .into_iter()
        .map(|s| Candidate {
            text: s.id.clone(),
            detail: format!(
                "{}{}  {}",
                s.agent,
                if s.ticket.is_some() { " (worker)" } else { "" },
                s.preview.as_deref().unwrap_or("")
            ),
        })
        .collect()
}

/// Agent definitions: the pages of `site://agents`.
fn agents(arg: &str, state: &AppState) -> Vec<Candidate> {
    state
        .catalog
        .pages
        .iter()
        .filter_map(|page| {
            let name = page
                .url
                .strip_prefix("site://agents/")?
                .strip_suffix(".md")?;
            (name != "index" && !name.contains('/') && starts_with(name, arg)).then(|| Candidate {
                text: name.to_string(),
                detail: page.title.clone(),
            })
        })
        .collect()
}

/// Sites until a `/` is typed, then that site's pages (`memory/user`, without `.md`).
fn pages(arg: &str, state: &AppState) -> Vec<Candidate> {
    let scheme = if arg.starts_with("site://") {
        "site://"
    } else {
        ""
    };
    let arg = &arg[scheme.len()..];
    if !arg.contains('/') {
        return state
            .catalog
            .sites
            .iter()
            .filter(|site| starts_with(&site.name, arg))
            .map(|site| Candidate {
                text: format!("{scheme}{}", site.name),
                detail: site.title.clone(),
            })
            .collect();
    }
    let mut items: Vec<Candidate> = state
        .catalog
        .pages
        .iter()
        .filter_map(|page| {
            let path = page_path(&page.url)?;
            starts_with(&path, arg).then(|| Candidate {
                text: format!("{scheme}{path}"),
                detail: page.title.clone(),
            })
        })
        .collect();
    items.sort_by(|a, b| a.text.cmp(&b.text));
    items.dedup_by(|a, b| a.text == b.text);
    items
}

/// `site://memory/user.md` → `memory/user`; an index is its directory (`memory`, `memory/topics`).
fn page_path(url: &str) -> Option<String> {
    let path = url.strip_prefix("site://")?;
    let path = path.strip_suffix(".md").unwrap_or(path);
    Some(path.strip_suffix("/index").unwrap_or(path).to_string())
}

fn app_sites(arg: &str, state: &AppState) -> Vec<Candidate> {
    state
        .catalog
        .sites
        .iter()
        .filter(|site| site.has_ui && starts_with(&site.name, arg))
        .map(|site| Candidate {
            text: site.name.clone(),
            detail: site.title.clone(),
        })
        .collect()
}

/// The project path is on the server's machine: complete it only when that is this one.
fn is_local(server_url: &str) -> bool {
    let host = server_url
        .split("://")
        .nth(1)
        .unwrap_or(server_url)
        .split(['/', ':'])
        .next()
        .unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost" | "[")
}

/// As the server reads it: `~/...` and `/...` are paths; anything else (`comp`, `work/api`)
/// is inside the projects directory.
fn projects(arg: &str, projects_dir: Option<&str>) -> Vec<Candidate> {
    let is_path = arg.starts_with(['~', '/']);
    let Some(dir) = projects_dir.filter(|_| !is_path) else {
        return directories(arg);
    };
    let base = format!("{}/", dir.trim_end_matches('/'));
    directories(&format!("{base}{arg}"))
        .into_iter()
        .filter_map(|c| {
            let name = c
                .text
                .strip_prefix(&base)?
                .trim_end_matches('/')
                .to_string();
            Some(Candidate {
                detail: format!("{base}{name}"),
                text: name,
            })
        })
        .collect()
}

/// Directories, like vim's file completion: `~/Pro` → `~/Projects/`. Hidden ones only after a `.`.
fn directories(arg: &str) -> Vec<Candidate> {
    let home = std::env::var("HOME").unwrap_or_default();
    let expanded = match arg.strip_prefix('~') {
        Some(rest) if !home.is_empty() => format!("{home}{rest}"),
        _ => arg.to_string(),
    };
    let (dir, partial) = match expanded.rfind('/') {
        Some(i) => (&expanded[..=i], &expanded[i + 1..]),
        None => ("./", expanded.as_str()),
    };
    // What the user typed before the partial name, `~` and all.
    let typed_dir = &arg[..arg.rfind('/').map_or(0, |i| i + 1)];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            name.starts_with(partial) && (partial.starts_with('.') || !name.starts_with('.'))
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| Candidate {
            text: format!("{typed_dir}{name}/"),
            detail: String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use composer_protocol::http::{SearchHit, SiteSummary};
    use composer_protocol::{SessionInfo, TurnState};

    use super::*;
    use crate::store::types::{Catalog, Startup};

    fn state() -> AppState {
        let mut state = AppState::new(Startup::default(), 160_000);
        let session = |id: &str, agent: &str, preview: &str| SessionInfo {
            id: id.into(),
            agent: agent.into(),
            project: None,
            ticket: None,
            parent: None,
            worker: None,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            preview: Some(preview.into()),
            updated: String::new(),
        };
        state.session.sessions = vec![
            session("a3f1", "default", "fix the login bug"),
            session("b2c3", "orchestrator", "plan the release"),
        ];
        let site = |name: &str, title: &str, has_ui: bool| SiteSummary {
            name: name.into(),
            title: title.into(),
            has_justfile: false,
            has_ui,
            app_built: has_ui,
            description: String::new(),
            pages: 0,
        };
        let page = |url: &str, title: &str| SearchHit {
            url: url.into(),
            title: title.into(),
            site: url
                .trim_start_matches("site://")
                .split('/')
                .next()
                .unwrap()
                .into(),
        };
        state.catalog = Catalog {
            sites: vec![
                site("memory", "Memory", false),
                site("orchestration", "Orchestration", true),
                site("agents", "Agents", false),
            ],
            pages: vec![
                page("site://memory/index.md", "Memory"),
                page("site://memory/user.md", "User"),
                page("site://memory/topics/rust.md", "Rust"),
                page("site://agents/index.md", "Agents"),
                page("site://agents/default.md", "Default agent"),
                page("site://agents/orchestrator.md", "Orchestrator"),
            ],
        };
        state
    }

    fn texts(line: &str) -> Vec<String> {
        complete(line, &state())
            .items
            .into_iter()
            .map(|c| c.text)
            .collect()
    }

    #[test]
    fn command_names_with_their_help() {
        assert_eq!(texts("a"), ["attach", "app"]);
        // One per command, however many forms it has.
        let all = texts("");
        let mut unique = all.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(all.len(), unique.len(), "{all:?}");
        assert_eq!(texts("pro"), ["project"]);
        assert_eq!(texts("wor"), ["workers", "work"]);
        let work = &complete("work", &state()).items[1];
        assert_eq!(
            work.detail,
            "new <pipeline> <name> [project] · run [work] [stage]"
        );
        assert_eq!(texts("work "), ["new", "run"]);
        assert_eq!(texts("work r"), ["run"]);
        assert!(texts("work run ").is_empty());
        let first = &complete("comp", &state()).items[0];
        assert_eq!(first.text, "compact");
        assert!(first.detail.contains("summarize"));
    }

    #[test]
    fn arguments_start_after_the_command() {
        let completions = complete("attach  a", &state());
        assert_eq!(completions.start, 8);
        assert_eq!(texts("attach a"), ["a3f1"]);
        assert_eq!(texts("attach "), ["a3f1", "b2c3"]);
        // By words of the last message too.
        assert_eq!(texts("attach release"), ["b2c3"]);
    }

    #[test]
    fn agents_from_the_agents_site() {
        assert_eq!(texts("new "), ["default", "orchestrator"]);
        assert_eq!(texts("new orch"), ["orchestrator"]);
    }

    #[test]
    fn sites_then_their_pages() {
        assert_eq!(texts("open mem"), ["memory"]);
        assert_eq!(texts("open memory/"), ["memory/topics/rust", "memory/user"]);
        assert_eq!(texts("open memory/u"), ["memory/user"]);
        assert_eq!(texts("o site://memory/t"), ["site://memory/topics/rust"]);
        assert_eq!(texts("app "), ["orchestration"]);
    }

    #[test]
    fn nothing_for_commands_without_arguments() {
        assert!(texts("compact ").is_empty());
        assert!(texts("frobnicate x").is_empty());
    }

    #[test]
    fn a_project_can_be_unset() {
        let mut state = state();
        state.conn.server_url = "https://home.tail1234.ts.net".into();
        assert!(
            complete("project n", &state).items.is_empty(),
            "no project yet"
        );
        state.session.sessions[0].project = Some("/home/u/Projects/composer".into());
        state.session.current = Some("a3f1".into());
        let items = complete("project n", &state).items;
        assert_eq!(
            (items[0].text.as_str(), items[0].detail.as_str()),
            ("none", "unset /home/u/Projects/composer")
        );
    }

    #[test]
    fn project_directories_only_for_a_local_server() {
        assert!(is_local("http://127.0.0.1:7420"));
        assert!(is_local("http://localhost:7420"));
        assert!(!is_local("https://home.tail1234.ts.net"));
        let dir = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta", ".hidden"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }
        std::fs::write(dir.path().join("file.txt"), "").unwrap();
        let base = format!("{}/", dir.path().display());
        let names: Vec<String> = directories(&base).into_iter().map(|c| c.text).collect();
        assert_eq!(names, [format!("{base}alpha/"), format!("{base}beta/")]);
        assert_eq!(directories(&format!("{base}.")).len(), 1);
        assert_eq!(
            directories(&format!("{base}a"))[0].text,
            format!("{base}alpha/")
        );

        // Names come from the projects directory; paths still complete as paths.
        let projects_dir = Some(dir.path().to_str().unwrap());
        let names: Vec<(String, String)> = projects("b", projects_dir)
            .into_iter()
            .map(|c| (c.text, c.detail))
            .collect();
        assert_eq!(names, [("beta".to_string(), format!("{base}beta"))]);
        assert_eq!(projects(&base, projects_dir).len(), 2);
        assert_eq!(projects("b", None), directories("b"));
    }
}
