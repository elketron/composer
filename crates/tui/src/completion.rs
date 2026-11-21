//! Completing `:` commands with Tab, as vim's command line does: command names, then each
//! command's argument (session ids, agents, sites and pages, models).

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
        "site" => all_sites(arg, state),
        "model" | "m" => models(arg, state),
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

/// One candidate per command. A command with several forms lists them as its detail.
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

/// By id; when no id starts with it, by words in the agent or the last message. Newest first.
/// Top-level sessions.
fn sessions(arg: &str, state: &AppState) -> Vec<Candidate> {
    let all: Vec<_> = state
        .session
        .sessions
        .iter()
        .filter(|s| s.parent.is_none())
        .collect();
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
            detail: format!("{}  {}", s.agent, s.preview.as_deref().unwrap_or("")),
        })
        .collect()
}

/// The provider's models by id or name; `none` when the session has its own.
fn models(arg: &str, state: &AppState) -> Vec<Candidate> {
    let needle = arg.to_lowercase();
    let unset = state
        .session
        .current_info()
        .and_then(|s| s.model.as_deref())
        .filter(|_| starts_with("none", arg))
        .map(|model| Candidate {
            text: "none".into(),
            detail: format!(
                "back to the server's {} (not {model})",
                state.catalog.default_model
            ),
        });
    let listed = state.catalog.models.iter().filter(|m| {
        m.id.to_lowercase().contains(&needle)
            || m.name
                .as_deref()
                .is_some_and(|n| n.to_lowercase().contains(&needle))
    });
    unset
        .into_iter()
        .chain(listed.map(|m| {
            Candidate {
                text: m.id.clone(),
                detail: [
                    m.name.clone(),
                    m.context.map(|c| format!("{}k context", c / 1000)),
                    (m.id == state.catalog.default_model).then(|| "the server's".into()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
            }
        }))
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

/// Every site; its `index.md` says what it brings.
fn all_sites(arg: &str, state: &AppState) -> Vec<Candidate> {
    let arg = arg.strip_prefix("site://").unwrap_or(arg);
    state
        .catalog
        .sites
        .iter()
        .filter(|site| starts_with(&site.name, arg))
        .map(|site| Candidate {
            text: site.name.clone(),
            detail: site.title.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use composer_protocol::http::{SearchHit, SiteSummary};
    use composer_protocol::{ModelInfo, SessionInfo, TurnState};

    use super::*;
    use crate::store::types::{Catalog, Startup};

    fn state() -> AppState {
        let mut state = AppState::new(Startup::default(), 160_000);
        let session = |id: &str, agent: &str, preview: &str| SessionInfo {
            id: id.into(),
            agent: agent.into(),
            project: None,
            site: None,
            parent: None,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            model: None,
            preview: Some(preview.into()),
            updated: String::new(),
        };
        state.session.sessions = vec![
            session("a3f1", "default", "fix the login bug"),
            session("b2c3", "default", "plan the release"),
            SessionInfo {
                parent: Some("b2c3".into()),
                ..session("c4d5", "default", "a spawned run")
            },
        ];
        let site = |name: &str, title: &str| SiteSummary {
            name: name.into(),
            title: title.into(),
            has_justfile: false,
            description: String::new(),
            pages: 0,
            mcp: Vec::new(),
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
                site("memory", "Memory"),
                site("plans", "Plans"),
                site("agents", "Agents"),
            ],
            pages: vec![
                page("site://memory/index.md", "Memory"),
                page("site://memory/user.md", "User"),
                page("site://memory/topics/rust.md", "Rust"),
                page("site://agents/index.md", "Agents"),
                page("site://agents/default.md", "Default"),
                page("site://agents/writer.md", "Writer"),
            ],
            models: vec![
                ModelInfo {
                    id: "qwen3-coder".into(),
                    name: None,
                    context: None,
                },
                ModelInfo {
                    id: "anthropic/claude-sonnet-4.5".into(),
                    name: Some("Anthropic: Claude Sonnet 4.5".into()),
                    context: Some(1_000_000),
                },
            ],
            default_model: "qwen3-coder".into(),
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
        assert_eq!(texts("a"), ["attach"]);
        // One per command, however many forms it has.
        let all = texts("");
        let mut unique = all.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(all.len(), unique.len(), "{all:?}");
        assert_eq!(texts("sc"), ["scope"]);
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
        // Not the runs it spawned.
        assert!(texts("attach c4").is_empty());
    }

    #[test]
    fn models_by_id_or_name() {
        assert_eq!(
            texts("model "),
            ["qwen3-coder", "anthropic/claude-sonnet-4.5"]
        );
        assert_eq!(texts("model sonnet"), ["anthropic/claude-sonnet-4.5"]);
        assert_eq!(texts("model Anthropic:"), ["anthropic/claude-sonnet-4.5"]);
        let items = complete("model ", &state()).items;
        assert_eq!(items[0].detail, "the server's");
        assert_eq!(
            items[1].detail,
            "Anthropic: Claude Sonnet 4.5 · 1000k context"
        );
    }

    #[test]
    fn agents_from_the_agents_site() {
        assert_eq!(texts("new "), ["default", "writer"]);
        assert_eq!(texts("new wr"), ["writer"]);
    }

    #[test]
    fn sites_then_their_pages() {
        assert_eq!(texts("open mem"), ["memory"]);
        assert_eq!(texts("open memory/"), ["memory/topics/rust", "memory/user"]);
        assert_eq!(texts("open memory/u"), ["memory/user"]);
        assert_eq!(texts("o site://memory/t"), ["site://memory/topics/rust"]);
        assert_eq!(texts("site "), ["memory", "plans", "agents"]);
        assert_eq!(texts("site ag"), ["agents"]);
    }

    #[test]
    fn nothing_for_commands_without_arguments() {
        assert!(texts("compact ").is_empty());
        assert!(texts("frobnicate x").is_empty());
    }
}
