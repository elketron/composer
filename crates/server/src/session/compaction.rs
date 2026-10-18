//! Compaction: when the context fills up, the agent first saves what matters to
//! memory, then older messages are replaced by a summary.

use std::sync::Arc;

use composer_protocol::ServerMessage;
use tokio_util::sync::CancellationToken;

use crate::agent::{AgentDefinition, system_prompt};
use crate::provider::{ModelMessage, ModelRequest};
use crate::server::Server;
use crate::session::Session;
use crate::session::model_call::{estimate_tokens, model_step, prepare_history, tool_step};
use crate::tools::ToolContext;

/// Tools the agent may use while saving memory.
const MEMORY_TOOLS: [&str; 4] = ["open", "search", "write", "edit"];
/// Model calls allowed for the memory pass.
const MEMORY_STEPS: usize = 8;

const MEMORY_PROMPT: &str = "[composer] The context is nearly full: older messages are about to be replaced by a summary.\n\
First save what is worth keeping beyond this conversation to site://memory, following its conventions: \
update existing pages instead of duplicating, write facts rather than transcripts, give feedback a Why and \
How to apply, use absolute dates, and keep the memory index's page list current. \
Only open, search, write and edit are available now. If nothing is worth saving, save nothing. \
When done, reply with one short line saying what you saved.";

const SUMMARY_PROMPT: &str = "[composer] Summarize the conversation so far, so that you can continue it without the older messages. \
Include: what the user wants and asked for, decisions made and why, the current state of the work, open questions \
and next steps, and the site:// pages and files involved. Be concise but complete: short sections and bullets. \
Write only the summary.";

/// Compacts the history. During a turn (`in_turn`) the turn in progress is kept as it is
/// and everything before it is summarized; between turns everything is summarized.
/// Returns how many messages were replaced (0 if there was nothing to compact).
pub async fn compact(
    server: &Arc<Server>,
    session: &Arc<Session>,
    cancel: &CancellationToken,
    in_turn: bool,
) -> Result<usize, String> {
    let history = session.history();
    let cut = if in_turn {
        history
            .iter()
            .rposition(|m| matches!(m, ModelMessage::User { .. }))
            .unwrap_or(0)
    } else {
        history.len()
    };
    if cut < 2 {
        return Ok(0);
    }
    let older = prepare_history(server, history[..cut].to_vec());
    let agent = AgentDefinition::load(&server.workspace, &session.info().agent)
        .map_err(|e| e.to_string())?;
    let system = ModelMessage::System {
        content: system_prompt(
            &agent,
            &server.workspace,
            &server.index.read().expect("index lock"),
            &session.info(),
        ),
    };

    save_memory(server, session, &agent, &system, &older, cancel).await?;
    if cancel.is_cancelled() {
        return Ok(0);
    }

    let mut messages = vec![system.clone()];
    messages.extend(older.iter().cloned());
    messages.push(ModelMessage::User {
        content: SUMMARY_PROMPT.into(),
        images: vec![],
    });
    let summary = model_step(
        server,
        session,
        ModelRequest {
            messages,
            tools: vec![],
        },
        cancel,
        false,
    )
    .await?;
    if summary.cancelled || summary.content.trim().is_empty() {
        return Err("compaction: the model returned no summary; nothing was changed".into());
    }
    let summary = summary.content.trim().to_string();

    let mut compacted = vec![
        ModelMessage::User {
            content: format!(
                "[composer] Summary of the conversation so far (older messages were compacted):\n\n{summary}"
            ),
            images: vec![],
        },
        ModelMessage::Assistant {
            content: "Understood, continuing from the summary.".into(),
            thinking: None,
            tool_calls: vec![],
        },
    ];
    compacted.extend(history[cut..].iter().cloned());
    session
        .replace_history(compacted.clone())
        .map_err(|e| e.to_string())?;

    let id = session.id().to_string();
    let messages = cut as u32;
    session.emit(&server.hub, |seq| ServerMessage::SessionCompacted {
        session: id,
        seq,
        summary,
        messages,
    });
    let estimate = estimate_tokens(&compacted) + estimate_tokens(std::slice::from_ref(&system));
    session.update_info(&server.hub, |info| info.context_tokens = estimate);
    Ok(cut)
}

/// The memory pass: a short tool loop with only the page tools, not added to history.
async fn save_memory(
    server: &Arc<Server>,
    session: &Arc<Session>,
    agent: &AgentDefinition,
    system: &ModelMessage,
    older: &[ModelMessage],
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut restricted = agent.clone();
    restricted.tools = Some(MEMORY_TOOLS.iter().map(|t| t.to_string()).collect());
    let restricted = Arc::new(restricted);
    let tools = server.tools.specs_for(&restricted);
    let context = ToolContext {
        server: server.clone(),
        session: session.clone(),
        agent: restricted,
        cancel: cancel.clone(),
    };

    let mut messages = vec![system.clone()];
    messages.extend(older.iter().cloned());
    messages.push(ModelMessage::User {
        content: MEMORY_PROMPT.into(),
        images: vec![],
    });
    for _ in 0..MEMORY_STEPS {
        let step = model_step(
            server,
            session,
            ModelRequest {
                messages: messages.clone(),
                tools: tools.clone(),
            },
            cancel,
            false,
        )
        .await?;
        if step.cancelled || step.tool_calls.is_empty() {
            return Ok(());
        }
        messages.push(ModelMessage::Assistant {
            content: step.content,
            thinking: None,
            tool_calls: step.tool_calls.clone(),
        });
        for call in &step.tool_calls {
            let (result, _) = tool_step(server, &context, call).await;
            messages.push(ModelMessage::Tool {
                call_id: call.id.clone(),
                content: result,
            });
        }
    }
    Ok(())
}
