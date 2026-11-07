//! The agent and the session's terminals (ticket 0176): it reads them; it types into one only
//! when the user approves (decision 2026-11-07, like recipe runs: approve, always for this
//! command, or deny).

use serde_json::Value;

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
    owned(&terminal, &context.session)?;
    Ok(terminal)
}
