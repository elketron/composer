use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use composer_protocol::ServerMessage;
use composer_protocol::http::OpenProject;
use tokio_util::sync::CancellationToken;

use crate::actions::{Caller, Justfile, RecipeEnv, run_recipe};
use crate::apps::{
    AppGrant, AttachReason, GrantedProject, ProjectName, ProjectWatch, attached_apps,
};
use crate::server::Server;

/// Output of an `on_open` recipe beyond this is cut in its event.
const MAX_OUTPUT: usize = 4 * 1024;

/// The projects open in a workspace (ticket 0165): how many clients have each, and its file
/// watch. The first open starts the watch and runs the attached apps' `on_open` recipes; the last
/// close stops the watch.
#[derive(Default)]
pub struct OpenProjects {
    open: Mutex<HashMap<ProjectName, Opened>>,
}

struct Opened {
    clients: u32,
    /// `None` when the watch couldn't start (reported; the project is open all the same).
    _watch: Option<ProjectWatch>,
}

impl OpenProjects {
    /// One more client has `name` (`folder`) open; `true` when it is the first.
    pub fn open(&self, server: &Arc<Server>, name: &ProjectName, folder: &Path) -> bool {
        let mut open = self.open.lock().expect("open projects lock");
        if let Some(opened) = open.get_mut(name) {
            opened.clients += 1;
            return false;
        }
        let watch = ProjectWatch::start(server, name, folder)
            .inspect_err(|e| tracing::warn!("project {name}: {e}"))
            .ok();
        open.insert(
            name.clone(),
            Opened {
                clients: 1,
                _watch: watch,
            },
        );
        let (server, name) = (server.clone(), name.to_string());
        tokio::spawn(async move { run_on_open(&server, &name).await });
        true
    }

    /// One client less; the last stops the watch.
    pub fn close(&self, name: &ProjectName) {
        let mut open = self.open.lock().expect("open projects lock");
        let Some(opened) = open.get_mut(name) else {
            return;
        };
        opened.clients -= 1;
        if opened.clients == 0 {
            open.remove(name);
        }
    }

    pub fn list(&self) -> Vec<OpenProject> {
        let open = self.open.lock().expect("open projects lock");
        let mut list: Vec<OpenProject> = open
            .iter()
            .map(|(name, o)| OpenProject {
                name: name.to_string(),
                clients: o.clients,
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

/// Runs the `on_open` recipes of the apps attached to `name`, each in the sandbox on the project
/// like an app's action, and reports each (`on_open.ran`). One that isn't approved for apps is
/// reported, not run; one that fails doesn't stop the rest.
async fn run_on_open(server: &Arc<Server>, name: &str) {
    let found = {
        let (server, name) = (server.clone(), name.to_string());
        tokio::task::spawn_blocking(move || attached_apps(&server, &name)).await
    };
    let Ok(Ok((project, folder, apps, _))) = found else {
        return;
    };
    let timeout = Duration::from_secs(server.config().agent.recipe_timeout_secs);
    let runnable = apps
        .iter()
        .filter(|a| a.why != AttachReason::LocalNotAllowed && !a.manifest.on_open.is_empty());
    for app in runnable {
        let grant = AppGrant {
            project: Some(GrantedProject {
                name: project.clone(),
                folder: folder.clone(),
            }),
            site: app.site.clone(),
            local: app.local.clone(),
        };
        let dir = app
            .local
            .clone()
            .unwrap_or_else(|| server.web.root().join(&app.site));
        let justfile = Justfile::load(&dir).await;
        for recipe in &app.manifest.on_open {
            let (status, output) = match &justfile {
                Err(e) => ("failed".to_string(), e.clone()),
                Ok(justfile) => run_one(server, &grant, justfile, recipe, timeout).await,
            };
            let mut output = output;
            if output.len() > MAX_OUTPUT {
                let mut end = MAX_OUTPUT;
                while !output.is_char_boundary(end) {
                    end -= 1;
                }
                output.truncate(end);
            }
            server.hub.send(ServerMessage::OnOpenRan {
                project: project.to_string(),
                app: app.site.clone(),
                recipe: recipe.clone(),
                status,
                output,
            });
        }
    }
}

async fn run_one(
    server: &Arc<Server>,
    grant: &AppGrant,
    justfile: &Justfile,
    recipe: &str,
    timeout: Duration,
) -> (String, String) {
    let Some(found) = justfile.recipe(recipe).filter(|r| !r.private) else {
        return (
            "failed".into(),
            format!("no recipe `{recipe}` in site://{}'s justfile", grant.site),
        );
    };
    let hash = justfile.hash(recipe);
    // A local app's allowance covers its justfile; a workspace app's recipes need approval.
    if grant.local.is_none()
        && !server
            .approvals
            .may_run(&grant.site, &found, &hash, Caller::App)
    {
        return (
            "needs approval".into(),
            format!(
                "`{recipe}` runs when {} opens once you approve it for apps (run it \
                 from the {} app once)",
                grant.place(),
                grant.site
            ),
        );
    }
    let env = RecipeEnv::for_app(server, grant);
    let ran = run_recipe(
        justfile,
        recipe,
        &[],
        &env,
        timeout,
        &CancellationToken::new(),
    )
    .await;
    match ran {
        Err(e) => ("failed".into(), e),
        Ok(out) if out.timed_out => ("timed out".into(), out.output),
        Ok(out) if out.exit_code == Some(0) => ("ok".into(), out.output),
        Ok(out) => ("failed".into(), out.output),
    }
}
