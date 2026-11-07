use async_trait::async_trait;
use serde_json::{Value, json};

use composer_protocol::{ApprovalDecision, ApprovalKind};

use crate::tools::terminal_tools::terminal;
use crate::tools::{Tool, ToolContext};

/// Types into a terminal of the session, with the user's approval (ticket 0176).
pub struct TerminalSendTool;

#[async_trait]
impl Tool for TerminalSendTool {
    fn name(&self) -> &'static str {
        "terminal_send"
    }

    fn description(&self) -> &'static str {
        "Type a command into a terminal on the user's stage (this session's), where they see it \
         run. The user approves each one (or always for that command). Then read the result with \
         `terminal_read`."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "the terminal's id" },
                "text": { "type": "string", "description": "the command" },
                "enter": { "type": "boolean", "description": "press Enter after it (default true)" }
            },
            "required": ["id", "text"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let terminal = terminal(context, &args)?;
        let text = args["text"].as_str().unwrap_or_default().to_string();
        if text.trim().is_empty() || text.contains('\u{1b}') {
            return Err("give the command as plain text".into());
        }
        let server = &context.server;
        // Rules per project, as for Claude Code's Bash: `Bash(<command>:*)`.
        let key = format!(
            "terminal:{}",
            context
                .session
                .info()
                .project
                .unwrap_or_else(|| "home".into())
        );
        let input = json!({ "command": text });
        if !server.tool_rules.allows(&key, "Bash", &input) {
            let rule = format!("Bash({}:*)", text.trim());
            let decision = server
                .ask_approval(
                    &context.session,
                    ApprovalKind::Tool,
                    format!("Type into terminal {}", terminal.id),
                    text.clone(),
                    Some(rule.clone()),
                    &context.cancel,
                )
                .await;
            match decision.map(|answer| (answer.decision, answer.reason)) {
                Some((ApprovalDecision::Approve, _)) => {}
                Some((ApprovalDecision::Always, _)) => {
                    if let Err(e) = server.tool_rules.add(&key, &rule) {
                        tracing::warn!("saving the rule {rule}: {e}");
                    }
                }
                Some((ApprovalDecision::Deny, reason)) => {
                    return Err(format!(
                        "The user didn't let you type `{text}`{}. Don't try to get around it.",
                        reason.map(|r| format!(": {r}")).unwrap_or_default()
                    ));
                }
                None => return Err("cancelled".into()),
            }
        }
        let enter = args["enter"].as_bool().unwrap_or(true);
        let typed = if enter {
            format!("{text}\r")
        } else {
            text.clone()
        };
        terminal.input(typed.as_bytes())?;
        Ok(format!(
            "typed `{text}` into terminal {}; read the result with `terminal_read`",
            terminal.id
        ))
    }
}
