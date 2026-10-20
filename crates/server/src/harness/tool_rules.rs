use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

/// Commands whose first word alone says little: their rule keeps the subcommand too
/// (`Bash(git push:*)`, not `Bash(git:*)`).
const SUBCOMMAND_TOOLS: &[&str] = &[
    "git", "cargo", "npm", "pnpm", "yarn", "npx", "bun", "docker", "kubectl", "go", "pip", "uv",
    "just", "dart", "flutter", "gh",
];

/// What the user said `always` to, per project: `state/tool_rules.json`,
/// `{ "<project folder>": ["Bash(npm test:*)", "WebFetch(domain:docs.rs)"] }`. Outside agent
/// reach. The rules are Claude Code's own permission syntax: they go to `--allowedTools` from
/// the next run on.
pub struct ToolRules {
    file: PathBuf,
    rules: Mutex<BTreeMap<String, Vec<String>>>,
}

impl ToolRules {
    pub fn load(file: PathBuf) -> Self {
        let rules = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            file,
            rules: Mutex::new(rules),
        }
    }

    /// The rules of `project`.
    pub fn rules(&self, project: &str) -> Vec<String> {
        let rules = self.rules.lock().expect("rules lock");
        rules.get(project).cloned().unwrap_or_default()
    }

    pub fn add(&self, project: &str, rule: &str) -> Result<(), String> {
        let mut rules = self.rules.lock().expect("rules lock");
        let entries = rules.entry(project.to_string()).or_default();
        if !entries.iter().any(|r| r == rule) {
            entries.push(rule.to_string());
        }
        let json = serde_json::to_string_pretty(&*rules).expect("rules serialize");
        std::fs::write(&self.file, json).map_err(|e| format!("{}: {e}", self.file.display()))
    }

    /// Whether one of `project`'s rules allows `tool` with `input`.
    pub fn allows(&self, project: &str, tool: &str, input: &Value) -> bool {
        self.rules(project)
            .iter()
            .any(|rule| rule_matches(rule, tool, input))
    }
}

/// The rule `always` adds for this call, if it has a safe one: `Bash(<command>:*)` for a plain
/// command (no shell operators), `WebFetch(domain:<host>)`, or the tool's name for tools that
/// don't touch files. File tools outside the working folders get none: allow them once.
pub fn rule_for(tool: &str, input: &Value) -> Option<String> {
    match tool {
        "Bash" => {
            let command = input["command"].as_str()?;
            if has_shell_operators(command) {
                return None;
            }
            let words: Vec<&str> = command.split_whitespace().collect();
            let first = *words.first()?;
            let prefix = if SUBCOMMAND_TOOLS.contains(&first) {
                // `git -C x push`: no subcommand to pin, and `git:*` would allow all of git.
                let sub = words.get(1).filter(|s| !s.starts_with('-'))?;
                format!("{first} {sub}")
            } else {
                first.to_string()
            };
            Some(format!("Bash({prefix}:*)"))
        }
        "WebFetch" => Some(format!(
            "WebFetch(domain:{})",
            host(input["url"].as_str()?)?
        )),
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "Glob" | "Grep" => None,
        _ => Some(tool.to_string()),
    }
}

/// Whether `rule` covers `tool` with `input` (the subset of Claude Code's syntax that
/// `rule_for` makes; anything else never matches here).
fn rule_matches(rule: &str, tool: &str, input: &Value) -> bool {
    let Some((name, pattern)) = rule.strip_suffix(')').and_then(|r| r.split_once('(')) else {
        return rule == tool;
    };
    if name != tool {
        return false;
    }
    match (tool, pattern.strip_suffix(":*")) {
        ("Bash", Some(prefix)) => input["command"].as_str().is_some_and(|command| {
            !has_shell_operators(command)
                && (command == prefix || command.starts_with(&format!("{prefix} ")))
        }),
        ("WebFetch", _) => pattern
            .strip_prefix("domain:")
            .is_some_and(|domain| input["url"].as_str().and_then(host).as_deref() == Some(domain)),
        _ => false,
    }
}

/// Whether the command chains, substitutes or redirects: anything that could run more than its
/// first word says. Single-quoted text is literal; in double quotes only `$` and `` ` `` count.
fn has_shell_operators(command: &str) -> bool {
    let mut quote = None;
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), _) => {}
            (Some(_), '$' | '`') => return true,
            (_, '\\') => {
                chars.next();
            }
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, ';' | '&' | '|' | '$' | '`' | '>' | '<' | '(' | ')' | '\n') => return true,
            (None, _) => {}
        }
    }
    // An unclosed quote: don't guess.
    quote.is_some()
}

fn host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_lowercase())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rules_from_calls() {
        let bash = |c: &str| rule_for("Bash", &json!({ "command": c }));
        assert_eq!(
            bash("python3 -c 'print(1)'").as_deref(),
            Some("Bash(python3:*)")
        );
        assert_eq!(
            bash("npm test -- --watch").as_deref(),
            Some("Bash(npm test:*)")
        );
        assert_eq!(bash("git -C x status"), None);
        assert_eq!(bash("make && rm -rf /"), None);
        assert_eq!(bash("echo $(whoami)"), None);
        assert_eq!(
            bash("python3 -c 'print(6*7); x = \"$y\"'").as_deref(),
            Some("Bash(python3:*)")
        );
        assert_eq!(bash("echo \"$(whoami)\""), None);
        assert_eq!(bash("echo 'unclosed"), None);
        assert_eq!(
            rule_for(
                "WebFetch",
                &json!({ "url": "https://Docs.rs/serde/latest" })
            )
            .as_deref(),
            Some("WebFetch(domain:docs.rs)")
        );
        assert_eq!(rule_for("Write", &json!({ "file_path": "/etc/x" })), None);
        assert_eq!(
            rule_for("WebSearch", &json!({})).as_deref(),
            Some("WebSearch")
        );
    }

    #[test]
    fn matching() {
        let bash = |c: &str| json!({ "command": c });
        assert!(rule_matches("Bash(npm test:*)", "Bash", &bash("npm test")));
        assert!(rule_matches(
            "Bash(npm test:*)",
            "Bash",
            &bash("npm test -- -u")
        ));
        assert!(!rule_matches(
            "Bash(npm test:*)",
            "Bash",
            &bash("npm testing")
        ));
        assert!(!rule_matches(
            "Bash(npm test:*)",
            "Bash",
            &bash("npm test; curl evil")
        ));
        assert!(!rule_matches("Bash(npm:*)", "Read", &bash("npm")));
        assert!(rule_matches(
            "WebFetch(domain:docs.rs)",
            "WebFetch",
            &json!({ "url": "https://docs.rs/x" })
        ));
        assert!(!rule_matches(
            "WebFetch(domain:docs.rs)",
            "WebFetch",
            &json!({ "url": "https://docs.rs.evil.com/x" })
        ));
        assert!(rule_matches("WebSearch", "WebSearch", &json!({})));
    }

    #[test]
    fn kept_per_project() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("tool_rules.json");
        let rules = ToolRules::load(file.clone());
        rules.add("/p", "Bash(python3:*)").unwrap();
        rules.add("/p", "Bash(python3:*)").unwrap();
        let again = ToolRules::load(file);
        assert_eq!(again.rules("/p"), ["Bash(python3:*)"]);
        assert!(again.allows("/p", "Bash", &json!({ "command": "python3 x.py" })));
        assert!(!again.allows("/q", "Bash", &json!({ "command": "python3 x.py" })));
    }
}
