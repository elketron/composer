//! The agent and the session's terminals (ticket 0176): it reads them; it types into one only
//! when the user approves (decision 2026-11-07, like recipe runs: approve, always for this
//! command, or deny). Background processes (`process_start`) are approved the same way.

use serde_json::{Value, json};

use composer_protocol::{ApprovalDecision, ApprovalKind};

use crate::actions::Ask;

use crate::terminals::{Terminal, owned};
use crate::tools::ToolContext;

/// Lines `terminal_read` gives by default, and at most.
pub(crate) const LINES: usize = 60;
pub(crate) const MAX_LINES: usize = 1000;

pub(crate) fn terminal(
    context: &ToolContext,
    args: &Value,
) -> Result<std::sync::Arc<Terminal>, String> {
    let id = args["id"].as_str().unwrap_or_default();
    let terminal = context.server.terminals.find(id)?;
    owned(&terminal, &*context.session()?)?;
    Ok(terminal)
}

/// The user's yes to running `command`: a rule they said `always` to in this folder, as for
/// Claude Code's Bash (`Bash(<command>:*)`), or the answer to `title` now (system-1's, or theirs).
pub(crate) async fn approve_command(
    context: &ToolContext,
    command: &str,
    title: String,
) -> Result<(), String> {
    let server = &context.server;
    let key = format!(
        "terminal:{}",
        context
            .site
            .as_ref()
            .and_then(|site| site.existing_folder(&server.approvals).ok())
            .map_or_else(|| "home".into(), |folder| folder.display().to_string())
    );
    let input = json!({ "command": command });
    if server.tool_rules.allows(&key, "Bash", &input) {
        return Ok(());
    }
    let rule = format!("Bash({}:*)", command.trim());
    let what = format!("`{}`", command.trim());
    let ask = Ask::action(
        ApprovalKind::Tool,
        what.clone(),
        title,
        command.to_string(),
        Some(rule.clone()),
    );
    let Some(answer) = server.ask(&*context.session()?, ask, &context.cancel).await else {
        return Err("cancelled".into());
    };
    match answer.decision {
        ApprovalDecision::Approve => Ok(()),
        ApprovalDecision::Always => {
            if let Err(e) = server.tool_rules.add(&key, &rule) {
                tracing::warn!("saving the rule {rule}: {e}");
            }
            Ok(())
        }
        ApprovalDecision::Deny => Err(answer.refusal(&what, || {
            format!("The user didn't let you run `{command}`. Don't try to get around it.")
        })),
    }
}
