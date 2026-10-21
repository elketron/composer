use async_trait::async_trait;
use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use serde_json::{Value, json};

use crate::pipeline::{NewWork, new_work};
use crate::tools::args::NewWorkArgs;
use crate::tools::{Tool, ToolContext};

pub struct NewWorkTool;

#[async_trait]
impl Tool for NewWorkTool {
    fn name(&self) -> &'static str {
        "new_work"
    }

    fn description(&self) -> &'static str {
        "Start a piece of work in a pipeline: the folder site://<pipeline>/<name>/ with its brief \
         (index.md) and a folder per stage. A work links a project (a folder on disk) when its \
         stages read or write code; a coding pipeline requires one. Returns the work's page."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pipeline": { "type": "string", "description": "the pipeline site, e.g. coding" },
                "name": { "type": "string", "description": "folder name: letters, digits, - or _, e.g. save-system" },
                "title": { "type": "string", "description": "shown in lists; defaults to the name" },
                "project": { "type": "string", "description": "a project name in the projects directory, or a path" },
                "brief": { "type": "string", "description": "what the work is for; the first stage reads it" }
            },
            "required": ["pipeline", "name"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: NewWorkArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let site = args
            .pipeline
            .trim()
            .trim_start_matches("site://")
            .trim_end_matches('/');
        context.check_write(&SiteUrl::new(site, "").map_err(|e| e.to_string())?)?;
        let work = new_work(
            &context.server,
            NewWork {
                pipeline: site,
                name: &args.name,
                title: args.title.as_deref(),
                project: args.project.as_deref(),
                brief: args.brief.as_deref(),
            },
            ChangedBy::Agent,
        )?;
        let project = work
            .project
            .as_deref()
            .map(|p| format!(" for {p}"))
            .unwrap_or_default();
        let first = work
            .pipeline
            .stages
            .first()
            .map(|s| format!(" Its first stage is {}.", s.name))
            .unwrap_or_default();
        Ok(format!("created {}{project}.{first}", work.url()))
    }
}
