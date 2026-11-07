use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::actions::{Justfile, RecipeEnv, run_recipe};
use crate::apps::app_build::{built_with, outdated};
use crate::server::Server;

/// The output of a rebuild kept per app, at most (its end: where errors are).
const TAIL: usize = 2_000;

/// The apps a rebuild would build: the workspace's sites with a `ui/` and a `ui-build` recipe,
/// those built with an older sdk or not built (`all`: every one); each with its recipe as the
/// user approves it.
pub async fn rebuild_plan(server: &Server, all: bool) -> Vec<(String, String)> {
    let mut plan = Vec::new();
    for site in server.web.sites().unwrap_or_default() {
        let dir = server.web.root().join(&site.name);
        if !dir.join("ui").is_dir() {
            continue;
        }
        let due = all || outdated(&dir) || built_with(&dir).is_none();
        if !due {
            continue;
        }
        let Ok(justfile) = Justfile::load(&dir).await else {
            continue;
        };
        if justfile.recipe("ui-build").is_none() {
            continue;
        }
        let shown = justfile.show("ui-build").await.unwrap_or_default();
        plan.push((site.name, shown));
    }
    plan
}

/// Runs each planned `ui-build` in the sandbox (as the site's recipe on Home): `(site, status,
/// the end of its output)`. The user's command is the approval.
pub async fn rebuild(server: &Arc<Server>, all: bool) -> Vec<(String, String, String)> {
    let timeout = Duration::from_secs(server.config().agent.recipe_timeout_secs);
    let mut done = Vec::new();
    for (site, _) in rebuild_plan(server, all).await {
        let dir = server.web.root().join(&site);
        let ran = match Justfile::load(&dir).await {
            Err(e) => Err(e),
            Ok(justfile) => {
                let env = RecipeEnv::new(server, &site, None);
                let cancel = CancellationToken::new();
                run_recipe(&justfile, "ui-build", &[], &env, timeout, &cancel).await
            }
        };
        let (status, output) = match ran {
            Err(e) => ("failed".to_string(), e),
            Ok(out) if out.timed_out => ("timed out".into(), out.output),
            Ok(out) if out.exit_code == Some(0) => ("built".into(), out.output),
            Ok(out) => ("failed".into(), out.output),
        };
        let start = output.len().saturating_sub(TAIL);
        let start = (start..output.len())
            .find(|i| output.is_char_boundary(*i))
            .unwrap_or(output.len());
        done.push((site, status, output[start..].to_string()));
    }
    done
}
