use async_trait::async_trait;
use serde_json::{Value, json};

use crate::pipeline::run_stage;
use crate::tools::args::RunStageArgs;
use crate::tools::{Tool, ToolContext};

pub struct RunStageTool;

#[async_trait]
impl Tool for RunStageTool {
    fn name(&self) -> &'static str {
        "run_stage"
    }

    fn description(&self) -> &'static str {
        "Run a stage of a pipeline work (site://<pipeline>/<work>): the next one not finished, or \
         `stage` (e.g. 02_plan, to run one again). It runs as a ticket worked by a stage worker \
         that sees only the stage's contract and its files; stages without a gate go on to the \
         next by themselves. Returns immediately; you get a message when the run stops for the \
         user (a gate to review, a failure, or the last stage done)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "work": { "type": "string", "description": "the work, e.g. coding/save-system" },
                "stage": { "type": "string", "description": "optional: the stage to run, e.g. 02_plan" }
            },
            "required": ["work"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: RunStageArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        if context.server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot run stages".into());
        }
        let started = run_stage(
            &context.server,
            &context.session,
            &args.work,
            args.stage.as_deref(),
        )
        .await?;
        let contract = started.work.pipeline.stage(&started.stage);
        let gate = if contract.is_some_and(|c| c.gate) {
            " It has a gate: it waits for the user's review when done."
        } else {
            " Without a gate, the next stage starts when it is done."
        };
        Ok(format!(
            "stage {} of {} started: ticket {}, worker #{}.{gate} Don't wait for it in this turn.",
            started.stage,
            started.work.url(),
            started.ticket,
            started.worker
        ))
    }
}
