use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::args::SetProjectArgs;
use crate::tools::{Tool, ToolContext};

pub struct SetProjectTool;

#[async_trait]
impl Tool for SetProjectTool {
    fn name(&self) -> &'static str {
        "set_project"
    }

    fn description(&self) -> &'static str {
        "Set the session's active project: a code directory on disk the user is working on. \
         Its memory page (site://memory/projects/<folder name>) is then loaded into your context. \
         An empty path clears it."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "absolute path or ~/..." } }
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: SetProjectArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let project = match args
            .path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            None => None,
            Some(path) => Some(resolve_project(path)?),
        };
        let message = match &project {
            Some(path) => format!("active project: {path}"),
            None => "active project cleared".to_string(),
        };
        context
            .session
            .update_info(&context.server.hub, |info| info.project = project);
        Ok(message)
    }
}

/// Expands `~`, requires an existing directory, returns the canonical path.
pub fn resolve_project(path: &str) -> Result<String, String> {
    let expanded = expand_home(path);
    let canonical = expanded
        .canonicalize()
        .map_err(|e| format!("{path}: {e}"))?;
    if !canonical.is_dir() {
        return Err(format!("{path} is not a directory"));
    }
    Ok(canonical.display().to_string())
}

/// `~` and `~/x` → under `$HOME`.
pub fn expand_home(path: &str) -> PathBuf {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
    };
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

/// The reverse, for showing paths: under `$HOME` → `~/x`.
pub fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}
