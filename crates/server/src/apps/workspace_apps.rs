use std::path::{Path, PathBuf};

use composer_core::{Page, SiteUrl};
use composer_protocol::http::{AppInfo, AppList};

use crate::apps::{AppManifest, AttachedApp, ProjectFacts, ProjectName};
use crate::harness::project_settings;
use crate::pipeline::Pipeline;
use crate::project::{ProjectPath, project_list};
use crate::server::Server;

/// The workspace's apps by site (sites with a `ui/` or an `app:` block), and the sites whose
/// block doesn't parse (`site://<site>: <what is wrong>`).
pub fn workspace_apps(server: &Server) -> (Vec<(String, AppManifest)>, Vec<String>) {
    let (mut apps, mut errors) = (Vec::new(), Vec::new());
    for site in server.web.sites().unwrap_or_default() {
        let app = index(server, &site.name).and_then(|p| p.frontmatter.get("app").cloned());
        match AppManifest::of_site(app.as_ref(), site.has_ui) {
            Ok(Some(manifest)) => apps.push((site.name, manifest)),
            Ok(None) => {}
            Err(e) => errors.push(format!("site://{}/index.md `app:`: {e}", site.name)),
        }
    }
    (apps, errors)
}

/// The workspace's projects by name: the folders it lists (its projects directory, folders its
/// sessions work in), named by folder or by `[projects."<path>"] name`. A second project with a
/// name already taken is left out (say so with `name` in its entry).
pub fn workspace_projects(server: &Server) -> Vec<(ProjectName, PathBuf)> {
    let config = server.config();
    let mut projects: Vec<(ProjectName, PathBuf)> = Vec::new();
    for summary in project_list(server, "") {
        let settings = project_settings(&config, &summary.folder);
        let folder = PathBuf::from(&summary.folder);
        let name = match settings.name.as_deref() {
            Some(name) => ProjectName::new(name),
            None => ProjectName::of_folder(&folder),
        };
        let Ok(name) = name else { continue };
        if projects.iter().any(|(taken, _)| *taken == name) {
            tracing::warn!(
                "two projects are called `{name}`; {} is left out of app URLs until its \
                 [projects] entry gives it a `name`",
                folder.display()
            );
            continue;
        }
        projects.push((name, folder));
    }
    projects
}

/// The project called `name` in this workspace.
pub fn find_project(server: &Server, name: &str) -> Result<(ProjectName, PathBuf), String> {
    let wanted = ProjectName::new(name)?;
    workspace_projects(server)
        .into_iter()
        .find(|(n, _)| *n == wanted)
        .ok_or_else(|| {
            format!(
                "no project `{name}` in this workspace: `GET /api/projects` lists them (a folder \
                 in the projects directory, or one a session works in)"
            )
        })
}

/// The apps of the project called `name`.
pub fn project_apps(server: &Server, name: &str) -> Result<AppList, String> {
    let (project, folder) = find_project(server, name)?;
    let config = server.config();
    let settings = project_settings(&config, &folder.display().to_string());
    let facts = ProjectFacts::gather(&folder, linking_pipelines(server, &folder), settings.tags);
    let (apps, errors) = workspace_apps(server);
    let attached = AttachedApp::for_project(&apps, &facts, &settings.apps, &settings.hide_apps);
    Ok(listing(server, Some(project.to_string()), attached, errors))
}

/// The workspace's Home: its global and both-scope apps.
pub fn home_apps(server: &Server) -> AppList {
    let (apps, errors) = workspace_apps(server);
    listing(server, None, AttachedApp::for_home(&apps), errors)
}

/// The pipelines (sites) with a work whose `project:` is `folder`.
fn linking_pipelines(server: &Server, folder: &Path) -> Vec<String> {
    let projects_dir = server.config().projects.dir.clone();
    let same = |linked: &str| {
        ProjectPath::resolve(linked, &projects_dir)
            .is_ok_and(|p| canonical(p.as_path()) == canonical(folder))
    };
    let sites = server.web.sites().unwrap_or_default();
    let pipelines = sites
        .iter()
        .filter_map(|s| Pipeline::from_dir(&s.name, &server.web.root().join(&s.name)));
    pipelines
        .filter(|pipeline| {
            pipeline.works().iter().any(|work| {
                index_of(server, &pipeline.name, &format!("{work}/index.md"))
                    .and_then(|p| p.frontmatter.get_str("project"))
                    .is_some_and(|linked| same(&linked))
            })
        })
        .map(|pipeline| pipeline.name)
        .collect()
}

fn listing(
    server: &Server,
    project: Option<String>,
    attached: Vec<AttachedApp>,
    errors: Vec<String>,
) -> AppList {
    let apps = attached
        .into_iter()
        .map(|app| {
            let title = index(server, &app.site).map(|p| p.title());
            AppInfo {
                label: app
                    .manifest
                    .rail
                    .label
                    .clone()
                    .or(title)
                    .unwrap_or_else(|| app.site.clone()),
                icon: app.manifest.rail.icon.clone(),
                scope: app.manifest.scope.to_string(),
                why: app.why.to_string(),
                tile: app.manifest.tile,
                opens: app.manifest.opens.iter().map(ToString::to_string).collect(),
                built: server
                    .web
                    .root()
                    .join(&app.site)
                    .join("ui/dist/index.html")
                    .is_file(),
                site: app.site,
            }
        })
        .collect();
    AppList {
        project,
        apps,
        errors,
    }
}

fn index(server: &Server, site: &str) -> Option<Page> {
    index_of(server, site, "index.md")
}

fn index_of(server: &Server, site: &str, path: &str) -> Option<Page> {
    server.web.read_page(&SiteUrl::new(site, path).ok()?).ok()
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
