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
        "Run a stage of a pipeline work (site://<pipeline>/<work>): the next one whose needs are \
         done, or `stage` (e.g. 02_plan, to run one again). It runs as a ticket worked by a stage \
         worker that sees only the stage's contract and its files. Returns immediately; the next \
         stage is another `run_stage` when this one is done (nothing starts it behind your back), \
         and you get a message when the run stops for the user (a gate to review, a failure, or \
         the last stage done)."
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
        let started = match run_stage(
            &context.server,
            &context.session,
            &args.work,
            args.stage.as_deref(),
            None,
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                if error.contains("is not a pipeline")
                    && let Some(suggestion) = suggest_work(context, &args.work)
                {
                    return Err(format!("{error}. Did you mean {suggestion}?"));
                }
                return Err(error);
            }
        };
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

/// A wrong `<pipeline>/<work>`: find the pipeline that actually has that work.
fn suggest_work(context: &ToolContext, spec: &str) -> Option<String> {
    let work = spec
        .trim()
        .trim_start_matches("site://")
        .trim_end_matches('/')
        .rsplit('/')
        .next()?;
    crate::pipeline::pipelines(&context.server)
        .into_iter()
        .find(|pipeline| pipeline.works().iter().any(|name| name == work))
        .map(|pipeline| format!("{}/{}", pipeline.name, work))
}
