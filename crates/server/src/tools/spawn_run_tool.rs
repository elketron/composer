use async_trait::async_trait;
use composer_protocol::RunKind;
use serde_json::{Value, json};

use crate::runs::{RunRequest, StoredRun};
use crate::tools::{Tool, ToolContext};

/// Starts another run from an agentic one: a single shot or a workflow run gives its value back
/// here; an agentic run becomes a session of its own.
pub struct SpawnRunTool;

#[async_trait]
impl Tool for SpawnRunTool {
    fn name(&self) -> &'static str {
        "spawn_run"
    }

    fn description(&self) -> &'static str {
        "Start another run and give it `prompt` as its input. `single_shot` (one model call, the \
         folder's instructions as its whole prompt, no tools) and `workflow` (reads pages, \
         changes nothing, returns a value) run now and return their result here. `agentic` \
         starts a new session that you spawned, with every tool, and returns its id at once; \
         when its run ends, its last answer is posted here as a message. With `wait: true` an \
         agentic run is waited for and its answer returned instead. `site` (default: yours) and \
         `folder`, a workflow folder inside that site, say whose instructions it follows. \
         `harness` (agentic only) runs it on an outside agent from config.toml's [acp.<name>]."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["single_shot", "workflow", "agentic"]
                },
                "site": { "type": "string", "description": "the site it runs in; default: yours" },
                "folder": {
                    "type": "string",
                    "description": "a workflow folder inside the site, e.g. krea2_prompt_improver"
                },
                "prompt": { "type": "string", "description": "its input" },
                "harness": {
                    "type": "string",
                    "description": "agentic only: the outside agent that runs it; default: composer"
                },
                "wait": {
                    "type": "boolean",
                    "description": "agentic only: wait for its answer instead of a message later"
                }
            },
            "required": ["kind", "prompt"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let kind: RunKind = serde_json::from_value(args["kind"].clone())
            .map_err(|_| "`kind` is single_shot, workflow or agentic".to_string())?;
        let prompt = args["prompt"]
            .as_str()
            .filter(|p| !p.trim().is_empty())
            .ok_or("give the run's input as `prompt`")?;
        let site = args["site"]
            .as_str()
            .map(str::to_string)
            .or_else(|| context.run.site());
        let request = RunRequest {
            kind,
            site,
            folder: args["folder"].as_str().map(str::to_string),
            input: prompt.to_string(),
            harness: args["harness"].as_str().map(str::to_string),
        }
        .checked(&context.server.web, &context.server.config())?;

        if kind != RunKind::Agentic {
            let value = request
                .run_unstored(&context.server, &context.cancel)
                .await?;
            return Ok(match value {
                Value::String(text) => text,
                other => other.to_string(),
            });
        }
        let wait = args["wait"].as_bool().unwrap_or(false);
        let parent = context.run.session().map(|s| s.id().to_string());
        if parent.is_none() && !wait {
            return Err(
                "this run is not stored, so nothing could hear when an agentic run ends: \
                 spawn it with `wait: true`"
                    .into(),
            );
        }
        let run = StoredRun::start(&context.server, &request, parent, wait)?;
        let id = run.session.id().to_string();
        if !wait {
            return Ok(format!(
                "started session {id}; when its run ends, its last answer is posted here"
            ));
        }
        let session = run.session.clone();
        tokio::select! {
            result = run.result() => {
                let value = result.map_err(|e| format!("session {id} stopped: {e}"))?;
                Ok(match value {
                    Value::String(text) => format!("session {id} answered:\n{text}"),
                    other => format!("session {id} answered:\n{other}"),
                })
            }
            _ = context.cancel.cancelled() => {
                session.cancel(None);
                Err(format!("cancelled; session {id} was stopped"))
            }
        }
    }
}
