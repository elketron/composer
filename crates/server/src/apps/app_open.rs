use composer_core::SiteUrl;
use composer_protocol::http::AppOpened;

use crate::api::encode_segment;
use crate::apps::{
    AppGrant, AppManifest, AttachReason, GrantedProject, attached_apps, find_project_by,
};
use crate::server::Server;

/// Opens `site`'s app on `project` (`None`: on Home): the URL that loads it, with what it needs
/// once in the URL. On a project and on Home an app gets a token of its own (`app_token`); a
/// client app on Home (`client: true`, the chat app) gets a one-time code that becomes the
/// workspace's cookie (`once`): never the workspace token itself.
pub fn open_app(server: &Server, site: &str, project: Option<&str>) -> Result<AppOpened, String> {
    let base = server.route_base();
    let Some(name) = project else {
        let manifest = home_manifest(server, site)?;
        if manifest.client {
            let code = server.once_codes.mint();
            return Ok(AppOpened {
                url: format!("{base}/{}/?once={code}", encode_segment(site)),
                token: None,
            });
        }
        let token = server.app_tokens.mint(AppGrant {
            project: None,
            site: site.to_string(),
            local: None,
        });
        return Ok(AppOpened {
            url: format!("{base}/h/{}/?app_token={token}", encode_segment(site)),
            token: Some(token),
        });
    };
    // By its name or its folder (a client knows its session's project as a folder).
    let (found, _) = find_project_by(server, name)?;
    let (project, folder, apps, _) = attached_apps(server, found.as_str())?;
    let Some(app) = apps.into_iter().find(|a| a.site == site) else {
        return Err(format!(
            "site://{site} isn't one of project `{name}`'s apps: its `app:` block's `when` doesn't \
             match it; pin it with `apps = [\"{site}\"]` in the project's [projects] entry, or \
             see `GET /api/projects/{name}/apps`"
        ));
    };
    if app.why == AttachReason::LocalNotAllowed {
        return Err(format!(
            "`{site}` is a project-local app of `{name}` (in its .composer/apps/) that you haven't \
             allowed, or that changed since: look at it, then run `composer app allow {name} {site}`"
        ));
    }
    let token = server.app_tokens.mint(AppGrant {
        project: Some(GrantedProject {
            name: project.clone(),
            folder,
        }),
        site: site.to_string(),
        local: app.local,
    });
    Ok(AppOpened {
        url: format!(
            "{base}/p/{}/{}/?app_token={token}",
            encode_segment(project.as_str()),
            encode_segment(site)
        ),
        token: Some(token),
    })
}

/// `site`'s manifest if it is a Home app (global or both).
fn home_manifest(server: &Server, site: &str) -> Result<AppManifest, String> {
    let manifest = site_manifest(server, site)?
        .ok_or_else(|| format!("site://{site} has no app (no `ui/` and no `app:` block)"))?;
    if !manifest.scope.on_home() {
        return Err(format!(
            "site://{site} is a project app (`scope: project`): open it from a project"
        ));
    }
    Ok(manifest)
}

/// `site`'s manifest (`None`: not an app); a broken `app:` block is the error.
pub fn site_manifest(server: &Server, site: &str) -> Result<Option<AppManifest>, String> {
    let url = SiteUrl::new(site, "").map_err(|e| e.to_string())?;
    let dir = server.web.root().join(url.site());
    if !dir.is_dir() {
        return Err(format!("no site://{site}"));
    }
    let page = server.web.read_page(&url).ok();
    let app = page.as_ref().and_then(|p| p.frontmatter.get("app"));
    AppManifest::of_site(app, dir.join("ui").is_dir())
        .map_err(|e| format!("site://{site}/index.md `app:`: {e}"))
}
