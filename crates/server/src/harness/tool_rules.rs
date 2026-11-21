use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

/// What the user said `always` to, per project: `state/tool_rules.json`,
/// `{ "<folder>": ["Bash(npm test:*)", "<mcp tool>"] }`. Outside agent
/// reach. The rules use Claude Code's permission syntax (`Bash(<command>:*)`, an MCP tool's
/// name).
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

/// Whether `rule` covers `tool` with `input` (`Bash(<prefix>:*)` or a tool's name; anything else
/// never matches here).
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
