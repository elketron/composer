use async_trait::async_trait;
use composer_protocol::{ApprovalDecision, ApprovalKind};
use serde_json::{Value, json};

use crate::library::{SiteName, WorkspaceLibrary};
use crate::tools::args::PublishSiteArgs;
use crate::tools::{Tool, ToolContext};

pub struct PublishSiteTool;

#[async_trait]
impl Tool for PublishSiteTool {
    fn name(&self) -> &'static str {
        "publish_site"
    }

    fn description(&self) -> &'static str {
        "Ask the user to publish a site of this workspace to the library, so other workspaces can \
         install it. Only its definition goes (index.md, justfile, ui/ source, stages/, _config/, \
         templates/ and what its index lists under `library:`), never works, tickets or other \
         pages. The user sees the files and the diff and decides each time; this waits for the \
         answer. Refused when the library's copy changed since this workspace's (update first)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "site": { "type": "string", "description": "a site of this workspace, e.g. cad" },
                "as": { "type": "string", "description": "its name in the library (default: the same)" }
            },
            "required": ["site"]
        })
    }

    async fn call(&self, context: &ToolContext, args: Value) -> Result<String, String> {
        let args: PublishSiteArgs = serde_json::from_value(args).map_err(|e| e.to_string())?;
        let server = &context.server;
        if server.parent.is_some() || context.session.info().ticket.is_some() {
            return Err("workers cannot publish sites; say in your Result what should be".into());
        }
        let site = SiteName::new(&args.site)?;
        let name = args.name.as_deref().map(SiteName::new).transpose()?;
        let library = WorkspaceLibrary::of(&server.home)?;
        let plan = library.plan_publish(&site, name.as_ref())?;
        if plan.changes.is_empty() {
            return Ok(format!(
                "nothing to publish: the library's `{}` is the same as site://{site}",
                plan.name
            ));
        }
        let title = format!("Publish site://{site} to the library as `{}`", plan.name);
        let answer = server
            .ask_approval(
                &context.session,
                ApprovalKind::Publish,
                title,
                plan.describe(),
                None,
                &context.cancel,
            )
            .await;
        match answer.map(|a| a.decision) {
            // `always` counts as this one yes: publishing is asked every time.
            Some(ApprovalDecision::Approve | ApprovalDecision::Always) => {
                let published = library.publish(&site, name.as_ref(), &plan.digest)?;
                Ok(format!(
                    "published site://{site} to the library as `{}` ({} file(s) changed)",
                    published.name,
                    published.changes.len()
                ))
            }
            Some(ApprovalDecision::Deny) => Err(
                "The user declined publishing it. Don't try another way; go on without it.".into(),
            ),
            None => Err("The turn was cancelled before the user answered.".into()),
        }
    }
}
