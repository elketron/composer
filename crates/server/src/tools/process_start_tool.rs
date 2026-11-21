use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::terminal_tools::approve_command;
use crate::tools::{Tool, ToolContext, expand_home};

/// Starts a command in the background, in a terminal of its own (the idea
/// `background-processes`): approved like `terminal_send`, its id back at once.
pub struct ProcessStartTool;

#[async_trait]
impl Tool for ProcessStartTool {
    fn name(&self) -> &'static str {
        "process_start"
    }

    fn description(&self) -> &'static str {
        "Start a long-running command in the background (a dev server, a watcher, a long build) \
         in a terminal of its own on the user's stage, and keep working: you get its terminal id \
         at once. Read its output with `terminal_read`; when it exits you get a message with its \
         exit code and last lines. It runs until you stop it (`process_stop`), the user does, or \
         composer stops: no time limit (wrap it in `timeout 10m ...` if you want one). The user \
         approves each command (or always for it). For a short command whose output you need \
         now, use a recipe instead."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "the shell command" },
                "cwd": {
                    "type": "string",
                    "description": "where it runs; default: the folder of the session's site. A relative path is inside that folder."
                }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let command = args["command"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        if command.is_empty() || command.contains('\u{1b}') {
            return Err("give the command as plain text".into());
        }
        let session = context.session()?;
        let server = &context.server;
        let home = context
            .site
            .as_ref()
            .and_then(|site| site.existing_folder().ok())
            .or_else(|| server.session_folder(&session.info()))
            .unwrap_or_else(|| server.home.sites());
        let cwd = match args["cwd"]
            .as_str()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            Some(cwd) => {
                let path = expand_home(cwd);
                let path = if path.is_absolute() {
                    path
                } else {
                    home.join(path)
                };
                if !path.is_dir() {
                    return Err(format!("{} is not a folder", path.display()));
                }
                path
            }
            None => home,
        };
        approve_command(
            context,
            &command,
            format!("Run in the background (in {})", cwd.display()),
        )
        .await?;
        let id = server
            .terminals
            .start_process(server, &session, &command, &cwd)?;
        Ok(format!(
            "started `{command}` in the background as terminal {id} (in {}). Read its output \
             with `terminal_read`; you'll get a message when it exits; `process_stop` ends it.",
            cwd.display()
        ))
    }
}
