use std::path::Path;

use crate::apps::{LocalApp, ProjectName, find_project_by};
use crate::server::Server;

/// `app.allow`: allows `app` of `project` as it is now; what was allowed.
pub fn allow_local(server: &Server, project: &str, app: &str) -> Result<String, String> {
    let (name, local) = local(server, project, app)?;
    server.local_apps.allow(&name, &local)?;
    Ok(format!(
        "allowed `{app}` of project `{name}` (its index.md and justfile as they are now; a change \
         turns it off until you allow it again)"
    ))
}

/// `app.promote`: copies `app` of `project` into the workspace's sites.
pub fn promote_local(server: &Server, project: &str, app: &str) -> Result<String, String> {
    let (name, local) = local(server, project, app)?;
    let target = server.web.root().join(&local.name);
    if target.exists() {
        return Err(format!(
            "the workspace already has site://{app}: rename the local app's folder, or remove the \
             site first"
        ));
    }
    copy(&local.dir, &target).map_err(|e| format!("copying {}: {e}", local.dir.display()))?;
    let attach = match local.manifest.when.is_empty() && local.manifest.scope.in_projects() {
        true => format!(
            " It has no `when`: pin it to `{name}` with `apps = [\"{app}\"]` in the project's \
             [projects] entry, or give its `app:` block a `when`."
        ),
        false => String::new(),
    };
    Ok(format!(
        "promoted `{app}` to site://{app}; the workspace's copy is the one used from now on.{attach}"
    ))
}

fn local(server: &Server, project: &str, app: &str) -> Result<(ProjectName, LocalApp), String> {
    let (name, folder) = find_project_by(server, project)?;
    let (apps, _) = LocalApp::discover(&folder);
    let local = apps.into_iter().find(|a| a.name == app).ok_or_else(|| {
        format!("project `{name}` has no local app `{app}` (in its .composer/apps/)")
    })?;
    Ok((name, local))
}

/// `from` into `to`, without dependencies (`node_modules`) or symlinks.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_symlink() || entry.file_name() == "node_modules" {
            continue;
        }
        if kind.is_dir() {
            copy(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
