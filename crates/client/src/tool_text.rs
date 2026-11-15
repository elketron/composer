//! What a tool call shows: its interesting argument on one line, and (for edits and to-do
//! lists) what it changes. Composer's own tools and Claude Code's.

use serde_json::Value;

/// Lines of a written file shown in the details.
const WRITE_LINES: usize = 40;

/// The interesting argument of a call, on one line.
pub fn tool_summary(name: &str, args: &Value) -> String {
    let get = |key: &str| args.get(key).and_then(|v| v.as_str());
    match name {
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            if let Some(path) = get("file_path").or(get("notebook_path")) {
                return short_path(path).to_string();
            }
        }
        "Bash" => {
            if let Some(command) = get("command") {
                return command.lines().next().unwrap_or_default().to_string();
            }
        }
        "Grep" | "Glob" => {
            if let Some(pattern) = get("pattern") {
                return match get("path") {
                    Some(path) => format!("{pattern} in {}", short_path(path)),
                    None => pattern.to_string(),
                };
            }
        }
        "Agent" | "Task" => {
            if let Some(description) = get("description") {
                return description.to_string();
            }
        }
        "Skill" => {
            if let Some(skill) = get("skill") {
                return skill.to_string();
            }
        }
        "TaskCreate" => {
            if let Some(subject) = get("subject") {
                return subject.to_string();
            }
        }
        "TaskUpdate" => {
            if let Some(id) = get("taskId") {
                return format!("#{id} {}", get("status").unwrap_or("updated"));
            }
        }
        "TodoWrite" => {
            let todos = args["todos"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            let done = todos.iter().filter(|t| t["status"] == "completed").count();
            return format!("{done}/{} done", todos.len());
        }
        _ => {}
    }
    summarize_args(args)
}

/// What the call changes, for the details: an edit's diff (`-` / `+` lines), a written file,
/// a to-do list. `None` for other calls.
pub fn tool_detail(name: &str, args: &Value) -> Option<String> {
    let get = |v: &Value, key: &str| v.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let diff = |edit: &Value| {
        let old = get(edit, "old_string").unwrap_or_default();
        let new = get(edit, "new_string").unwrap_or_default();
        let mut lines: Vec<String> = old.lines().map(|l| format!("- {l}")).collect();
        lines.extend(new.lines().map(|l| format!("+ {l}")));
        lines.join("\n")
    };
    match name {
        "Edit" => Some(diff(args)),
        "MultiEdit" => Some(
            args["edits"]
                .as_array()?
                .iter()
                .map(diff)
                .collect::<Vec<_>>()
                .join("\n\n"),
        ),
        "Write" => {
            let content = get(args, "content")?;
            let total = content.lines().count();
            let mut lines: Vec<String> = content
                .lines()
                .take(WRITE_LINES)
                .map(|l| format!("+ {l}"))
                .collect();
            if total > WRITE_LINES {
                lines.push(format!("… {} more lines", total - WRITE_LINES));
            }
            Some(lines.join("\n"))
        }
        "TodoWrite" => Some(
            args["todos"]
                .as_array()?
                .iter()
                .map(|t| {
                    let mark = match t["status"].as_str() {
                        Some("completed") => "[x]",
                        Some("in_progress") => "[~]",
                        _ => "[ ]",
                    };
                    format!("{mark} {}", t["content"].as_str().unwrap_or_default())
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// A path inside a ticket's worktree, from the worktree (`src/main.rs`); others as they are.
fn short_path(path: &str) -> &str {
    match path.find("/worktrees/") {
        Some(i) => {
            let rest = &path[i + "/worktrees/".len()..];
            rest.split_once('/').map_or(rest, |(_, inside)| inside)
        }
        None => path,
    }
}

/// Composer's tools: the URL, query, recipe… of the call.
fn summarize_args(args: &Value) -> String {
    let get = |key: &str| args.get(key).and_then(|v| v.as_str());
    if let (Some(site), Some(recipe)) = (get("site"), get("recipe")) {
        let extra: Vec<&str> = args["args"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str())
            .collect();
        return format!("{site} › {recipe} {}", extra.join(" "))
            .trim_end()
            .to_string();
    }
    for key in ["url", "query", "path", "ticket", "status", "site"] {
        if let Some(value) = get(key) {
            return value.to_string();
        }
    }
    match args {
        Value::Object(map) if map.is_empty() => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn summaries() {
        let wt = "/home/me/.composer/state/worktrees/0007/src/main.rs";
        assert_eq!(
            tool_summary("Edit", &json!({ "file_path": wt })),
            "src/main.rs"
        );
        assert_eq!(
            tool_summary("Bash", &json!({ "command": "cargo test\necho hi" })),
            "cargo test"
        );
        assert_eq!(
            tool_summary("Grep", &json!({ "pattern": "fn main", "path": wt })),
            "fn main in src/main.rs"
        );
        assert_eq!(
            tool_summary(
                "Agent",
                &json!({ "description": "Read a.txt", "prompt": "…" })
            ),
            "Read a.txt"
        );
        assert_eq!(
            tool_summary(
                "TaskUpdate",
                &json!({ "taskId": "1", "status": "completed" })
            ),
            "#1 completed"
        );
        assert_eq!(
            tool_summary(
                "TodoWrite",
                &json!({ "todos": [{ "content": "a", "status": "completed" }, { "content": "b", "status": "pending" }] })
            ),
            "1/2 done"
        );
        assert_eq!(
            tool_summary(
                "run",
                &json!({ "site": "tools", "recipe": "greet", "args": ["bob"] })
            ),
            "tools › greet bob"
        );
        assert_eq!(
            tool_summary("open", &json!({ "url": "site://home" })),
            "site://home"
        );
    }

    #[test]
    fn details() {
        assert_eq!(
            tool_detail(
                "Edit",
                &json!({ "old_string": "one", "new_string": "two\nthree" })
            )
            .as_deref(),
            Some("- one\n+ two\n+ three")
        );
        assert_eq!(
            tool_detail(
                "TodoWrite",
                &json!({ "todos": [{ "content": "a", "status": "completed" }, { "content": "b", "status": "in_progress" }] })
            )
            .as_deref(),
            Some("[x] a\n[~] b")
        );
        assert_eq!(tool_detail("Read", &json!({ "file_path": "x" })), None);
    }
}
