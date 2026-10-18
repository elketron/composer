use async_trait::async_trait;
use serde_json::{Value, json};

use crate::actions::Justfile;
use crate::tools::args::ActionsArgs;
use crate::tools::{Tool, ToolContext};

pub struct ActionsTool;

#[async_trait]
impl Tool for ActionsTool {
    fn name(&self) -> &'static str {
        "actions"
    }

    fn description(&self) -> &'static str {
        "List a site's actions: the recipes in its justfile, with their parameters and \
         whether the user already approved them. Run one with `run`."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "site": { "type": "string", "description": "site name, e.g. orchestration" } },
            "required": ["site"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: ActionsArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let site = args
            .site
            .trim_start_matches("site://")
            .trim_end_matches('/')
            .to_string();
        context.check_read(&context.url(&format!("site://{site}"))?)?;
        let justfile = Justfile::load(&context.server.workspace.root().join(&site)).await?;
        let mut lines = vec![format!("site://{site} actions (run with `run`):")];
        for recipe in justfile.recipes().into_iter().filter(|r| !r.private) {
            let approved = context.server.approvals.is_approved(
                &site,
                &recipe.name,
                &justfile.hash(&recipe.name),
            );
            let mut line = format!("- {}", recipe.name);
            for parameter in &recipe.parameters {
                line.push_str(&format!(" <{parameter}>"));
            }
            if let Some(doc) = &recipe.doc {
                line.push_str(&format!(" — {doc}"));
            }
            line.push_str(if approved {
                " [approved]"
            } else {
                " [needs the user's approval]"
            });
            lines.push(line);
        }
        if lines.len() == 1 {
            lines.push("(no public recipes)".into());
        }
        Ok(lines.join("\n"))
    }
}
