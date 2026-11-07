use std::path::{Path, PathBuf};

use composer_core::{Page, SiteUrl};
use composer_protocol::http::{AppInfo, AppList, ProjectInfo};

use crate::apps::{AppManifest, AttachReason, AttachedApp, LocalApp, ProjectFacts, ProjectName};
use crate::harness::project_settings;
use crate::pipeline::Pipeline;
use crate::project::{ProjectPath, project_list};
use crate::server::Server;

/// The workspace's apps by site (sites with a `ui/` or an `app:` block), and the sites whose
/// block doesn't parse (`site://<site>: <what is wrong>`) or that ask to be a client the user
/// hasn't listed.
pub fn workspace_apps(server: &Server) -> (Vec<(String, AppManifest)>, Vec<String>) {
    let (mut apps, mut errors) = (Vec::new(), Vec::new());
    for site in server.web.sites().unwrap_or_default() {
        let app = index(server, &site.name).and_then(|p| p.frontmatter.get("app").cloned());
        match AppManifest::of_site(app.as_ref(), site.has_ui) {
            Ok(Some(manifest)) => {
                if manifest.client && !crate::apps::is_client(server, &site.name) {
                    errors.push(format!(
                        "site://{} asks to be a client app (the workspace's full access: sessions, \
                         approvals); it opens with a token of its own until you add it to \
                         `[apps] clients` in config.toml",
                        site.name
                    ));
                }
                apps.push((site.name, manifest));
            }
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

/// The project a client names: by its name, else by its folder (`~/...`, absolute, or a name in
/// the projects directory).
pub fn find_project_by(
    server: &Server,
    name_or_path: &str,
) -> Result<(ProjectName, PathBuf), String> {
    if let Ok(found) = find_project(server, name_or_path) {
        return Ok(found);
    }
    let dir = server.config().projects.dir.clone();
    let folder = ProjectPath::resolve(name_or_path, &dir)
        .map(|p| canonical(p.as_path()))
        .map_err(|_| format!("no project `{name_or_path}` in this workspace"))?;
    workspace_projects(server)
        .into_iter()
        .find(|(_, f)| canonical(f) == folder)
        .ok_or_else(|| format!("no project `{name_or_path}` in this workspace"))
}

/// The apps attached to the project called `name`, with their manifests; and the sites whose
/// `app:` block doesn't parse.
pub fn attached_apps(
    server: &Server,
    name: &str,
) -> Result<(ProjectName, PathBuf, Vec<AttachedApp>, Vec<String>), String> {
    let (project, folder) = find_project(server, name)?;
    let config = server.config();
    let settings = project_settings(&config, &folder.display().to_string());
    let facts = ProjectFacts::gather(&folder, linking_pipelines(server, &folder), settings.tags);
    let (apps, mut errors) = workspace_apps(server);
    let mut attached = AttachedApp::for_project(&apps, &facts, &settings.apps, &settings.hide_apps);
    let (locals, local_errors) = LocalApp::discover(&folder);
    errors.extend(local_errors);
    for local in locals {
        if apps.iter().any(|(site, _)| *site == local.name)
            || server.web.root().join(&local.name).is_dir()
        {
            errors.push(format!(
                "the project-local app `{}` is shadowed by site://{} (the workspace's wins)",
                local.name, local.name
            ));
            continue;
        }
        let why = match server.local_apps.allows(&project, &local) {
            true => AttachReason::Local,
            false => AttachReason::LocalNotAllowed,
        };
        attached.push(AttachedApp {
            site: local.name.clone(),
            manifest: local.manifest.clone(),
            why,
            local: Some(local.dir.clone()),
        });
    }
    Ok((project, folder, attached, errors))
}

/// The apps of the project called `name`.
pub fn project_apps(server: &Server, name: &str) -> Result<AppList, String> {
    let (project, _, attached, errors) = attached_apps(server, name)?;
    Ok(listing(server, Some(project.to_string()), attached, errors))
}

/// The workspace's Home: its global and both-scope apps, each `tile` app with its tile.
pub fn home_apps(server: &Server) -> AppList {
    let (apps, errors) = workspace_apps(server);
    let mut list = listing(server, None, AttachedApp::for_home(&apps), errors);
    for app in list.apps.iter_mut().filter(|a| a.tile) {
        app.tile_text = index_of(server, &app.site, "tile.md").map(|p| p.body().trim().to_string());
    }
    list
}

/// What the sdk's `project()` returns for an app opened with `grant`.
pub fn project_info(server: &Server, project: &crate::apps::GrantedProject) -> ProjectInfo {
    let config = server.config();
    let folder = &project.folder;
    let settings = project_settings(&config, &folder.display().to_string());
    let pipelines = linking_pipelines(server, folder);
    let branch = crate::project::git::git(folder, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .filter(|b| !b.is_empty() && b != "HEAD");
    ProjectInfo {
        name: project.name.to_string(),
        root: folder.display().to_string(),
        branch: folder.join(".git").exists().then_some(branch).flatten(),
        pipeline: pipelines.first().cloned(),
        pipelines,
        tags: settings.tags,
    }
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
            let title = match &app.local {
                Some(dir) => local_title(&app.site, dir),
                None => index(server, &app.site).map(|p| p.title()),
            };
            let dir = app
                .local
                .clone()
                .unwrap_or_else(|| server.web.root().join(&app.site));
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
                built: dir.join("ui/dist/index.html").is_file(),
                outdated: crate::apps::outdated(&dir),
                client: app.local.is_none() && crate::apps::is_client(server, &app.site),
                local: app.local.is_some(),
                tile_text: None,
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

fn local_title(name: &str, dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("index.md")).ok()?;
    Some(Page::parse(SiteUrl::new(name, "index.md").ok()?, text).title())
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
