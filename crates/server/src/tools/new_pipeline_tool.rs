use async_trait::async_trait;
use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::pipeline::{new_pipeline, templates};
use crate::tools::args::NewPipelineArgs;
use crate::tools::{Tool, ToolContext};

pub struct NewPipelineTool;

#[async_trait]
impl Tool for NewPipelineTool {
    fn name(&self) -> &'static str {
        "new_pipeline"
    }

    fn description(&self) -> &'static str {
        "Create a pipeline: a site for one kind of staged work (video, research, coding, …) made \
         from a template in templates/pipelines/. It holds the stage contracts (stages/), shared \
         rules (_config/) and, as folders, the works. Once per kind; then `new_work` starts a piece \
         of work in it. Without `kind`, the error lists the templates."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "description": "the template, e.g. coding, video, research" },
                "name": { "type": "string", "description": "the site's name; defaults to the kind" }
            },
            "required": ["kind"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: NewPipelineArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let server = &context.server;
        let name = args
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        let site = name.unwrap_or(args.kind.trim());
        context.check_write(&SiteUrl::new(site, "").map_err(|e| e.to_string())?)?;
        let pipeline = new_pipeline(server, &args.kind, name, ChangedBy::Agent).map_err(|e| {
            if templates(server).is_empty() {
                format!("{e}; there are no pipeline templates (run `composer update`)")
            } else {
                e
            }
        })?;
        crate::pipeline::prepare_board(server, &pipeline.name);
        let stages: Vec<String> = pipeline
            .stages
            .iter()
            .map(|s| format!("{}{}", s.name, if s.gate { " (gate)" } else { "" }))
            .collect();
        Ok(format!(
            "created site://{} from the {} template: stages {}.\n\
             Next: `new_work` starts a piece of work in it.",
            pipeline.name,
            args.kind.trim(),
            stages.join(", ")
        ))
    }
}
