use std::time::Duration;

use async_trait::async_trait;
use composer_protocol::ApprovalDecision;
use serde_json::{Value, json};

use crate::actions::{Justfile, RecipeEnv, run_recipe};
use crate::tools::args::RunArgs;
use crate::tools::{Tool, ToolContext};

pub struct RunTool;

#[async_trait]
impl Tool for RunTool {
    fn name(&self) -> &'static str {
        "run"
    }

    fn description(&self) -> &'static str {
        "Run a site action (a justfile recipe) in the site's folder and get its output. \
         The first run of a recipe, and the first run after it changes, waits for the user's approval. \
         The recipe gets COMPOSER_PROJECT (the active project path), COMPOSER_SITE and COMPOSER_SESSION."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "site": { "type": "string" },
                "recipe": { "type": "string" },
                "args": { "type": "array", "items": { "type": "string" }, "description": "recipe arguments, in order" }
            },
            "required": ["site", "recipe"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: RunArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let site = args
            .site
            .trim_start_matches("site://")
            .trim_end_matches('/')
            .to_string();
        context.check_write(&context.url(&format!("site://{site}"))?)?;
        let server = &context.server;
        let site_dir = server.workspace.root().join(&site);

        let justfile = Justfile::load(&site_dir).await?;
        let recipe = justfile.recipe(&args.recipe).ok_or_else(|| {
            format!(
                "no recipe `{}` in site://{site}; see `actions`",
                args.recipe
            )
        })?;
        if recipe.private {
            return Err(format!("`{}` is a private helper recipe", recipe.name));
        }
        let hash = justfile.hash(&recipe.name);

        if !server.approvals.is_approved(&site, &recipe.name, &hash) {
            let body = justfile.show(&recipe.name).await?;
            let decision = server
                .request_approval(
                    &context.session,
                    &site,
                    &recipe.name,
                    &args.args,
                    body,
                    &hash,
                    &context.cancel,
                )
                .await;
            match decision {
                Some(ApprovalDecision::Approve | ApprovalDecision::Always) => server
                    .approvals
                    .approve(&site, &recipe.name, &hash)
                    .map_err(|e| e.to_string())?,
                Some(ApprovalDecision::Deny) => {
                    return Err(format!(
                        "the user denied running `{}`; ask them why before trying again",
                        recipe.name
                    ));
                }
                None => return Err("cancelled by the user".into()),
            }
            // The justfile may have changed while the user was looking: run only what they approved.
            let current = Justfile::load(&site_dir).await?;
            if current.hash(&recipe.name) != hash {
                return Err("the recipe changed while waiting for approval; run it again".into());
            }
        }

        let env = RecipeEnv {
            site: site.clone(),
            session: context.session.id().to_string(),
            project: context.session.info().project,
        };
        let timeout = Duration::from_secs(server.config.agent.recipe_timeout_secs);
        let output = run_recipe(
            &justfile,
            &recipe.name,
            &args.args,
            &env,
            timeout,
            &context.cancel,
        )
        .await?;
        let status = match (output.exit_code, output.timed_out) {
            (_, true) => "timed out".to_string(),
            (Some(0), _) => "ok".to_string(),
            (Some(code), _) => format!("failed (exit {code})"),
            (None, _) => "killed".to_string(),
        };
        let text = format!(
            "{} › {}: {status}\n{}",
            site,
            recipe.name,
            output.output.trim_end()
        );
        if output.exit_code == Some(0) {
            Ok(text)
        } else {
            Err(text)
        }
    }
}
