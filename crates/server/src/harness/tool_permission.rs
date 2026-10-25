//! Claude Code's `--permission-prompt-tool`: a tool outside its allowlist becomes a `tool`
//! approval in the worker's session, answered from any client.

use std::sync::Arc;
use std::time::Duration;

use composer_protocol::{ApprovalDecision, ApprovalKind, ToolInfo};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::harness::tool_rules::rule_for;
use crate::server::Server;

/// The tool's name in `composer mcp`; Claude Code calls it as `mcp__composer__permission`.
pub const PERMISSION_TOOL: &str = "permission";

/// Its listing, for external harnesses only.
pub fn permission_tool_info() -> ToolInfo {
    ToolInfo {
        name: PERMISSION_TOOL.into(),
        description: "Asks the user whether a tool outside the allowlist may run (called by \
                      Claude Code itself, never by the model)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "tool_name": { "type": "string" },
                "input": { "type": "object" },
                "tool_use_id": { "type": "string" }
            },
            "required": ["tool_name", "input"]
        }),
    }
}

/// Answers one permission request of the running worker `session`, as the JSON text Claude
/// Code expects: `{"behavior":"allow","updatedInput":…}` or `{"behavior":"deny","message":…}`.
pub async fn answer_permission(server: &Arc<Server>, session: &str, args: Value) -> String {
    let tool = args["tool_name"].as_str().unwrap_or_default().to_string();
    let input = args["input"].clone();
    let deny = |message: &str| json!({ "behavior": "deny", "message": message }).to_string();
    let allow = || json!({ "behavior": "allow", "updatedInput": input }).to_string();

    let Some(handle) = server.workers.get(session) else {
        return deny("not a running worker");
    };
    if tool.is_empty() || tool.ends_with(&format!("__{PERMISSION_TOOL}")) {
        return deny("not a tool composer can approve");
    }
    let project = handle.project.clone();
    if let Some(project) = &project
        && server.tool_rules.allows(project, &tool, &input)
    {
        return allow();
    }
    if !server.config().claude_code.ask {
        return deny("This tool is not in the allowlist; the user doesn't take questions here.");
    }
    let Ok(worker) = server.sessions.get(session) else {
        return deny("no worker session");
    };

    // Closed when the run ends (cancelled, timed out, …) before anyone answers.
    let cancel = CancellationToken::new();
    let watch = {
        let (server, session, cancel) = (server.clone(), session.to_string(), cancel.clone());
        tokio::spawn(async move {
            while server.workers.get(&session).is_some() {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            cancel.cancel();
        })
    };
    let rule = project.as_ref().and_then(|_| rule_for(&tool, &input));
    let decision = server
        .ask_approval(
            &worker,
            ApprovalKind::Tool,
            title(&tool, &input),
            body(&tool, &input),
            rule.clone(),
            &cancel,
        )
        .await;
    watch.abort();
    match decision {
        Some(ApprovalDecision::Approve) => allow(),
        Some(ApprovalDecision::Always) => {
            if let (Some(project), Some(rule)) = (&project, &rule)
                && let Err(e) = server.tool_rules.add(project, rule)
            {
                tracing::warn!("saving the tool rule {rule}: {e}");
            }
            allow()
        }
        Some(ApprovalDecision::Deny) => deny(
            "The user denied this. Don't try to get around it; do without, or say in the ticket's \
             Result what you needed and why.",
        ),
        None => deny("The run is ending."),
    }
}

/// `Bash: npm test`, `Write /etc/hosts`, `WebFetch https://…`, or the tool's name.
fn title(tool: &str, input: &Value) -> String {
    let detail = match tool {
        "Bash" => input["command"].as_str().map(|c| {
            let line = c.lines().next().unwrap_or_default();
            let cut: String = line.chars().take(80).collect();
            if cut.len() < c.len() {
                format!(": {cut}…")
            } else {
                format!(": {cut}")
            }
        }),
        _ => input["file_path"]
            .as_str()
            .or(input["url"].as_str())
            .or(input["notebook_path"].as_str())
            .map(|p| format!(" {p}")),
    };
    format!("Claude Code: {tool}{}", detail.unwrap_or_default())
}

/// What to look at: the command (and why), an edit's change, or the input.
fn body(tool: &str, input: &Value) -> String {
    match tool {
        "Bash" => {
            let command = input["command"].as_str().unwrap_or_default();
            match input["description"].as_str() {
                Some(why) if !why.is_empty() => format!("{command}\n\n# {why}"),
                _ => command.to_string(),
            }
        }
        "Edit" => format!(
            "{}\n\n- {}\n+ {}",
            input["file_path"].as_str().unwrap_or_default(),
            input["old_string"]
                .as_str()
                .unwrap_or_default()
                .replace('\n', "\n- "),
            input["new_string"]
                .as_str()
                .unwrap_or_default()
                .replace('\n', "\n+ "),
        ),
        "Write" => format!(
            "{}\n\n{}",
            input["file_path"].as_str().unwrap_or_default(),
            input["content"].as_str().unwrap_or_default()
        ),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_bodies() {
        let bash = json!({ "command": "python3 -c 'print(6*7)'", "description": "Multiply" });
        assert_eq!(
            title("Bash", &bash),
            "Claude Code: Bash: python3 -c 'print(6*7)'"
        );
        assert_eq!(body("Bash", &bash), "python3 -c 'print(6*7)'\n\n# Multiply");
        let write = json!({ "file_path": "/etc/hosts", "content": "x" });
        assert_eq!(title("Write", &write), "Claude Code: Write /etc/hosts");
        assert_eq!(
            title("WebSearch", &json!({ "query": "q" })),
            "Claude Code: WebSearch"
        );
    }
}
