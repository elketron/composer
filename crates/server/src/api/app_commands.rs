use composer_protocol::ClientCommand;

use crate::apps::AppGrant;

/// Whether an app's socket may send `command` (ticket 0163): subscribe to pages (it reads the
/// whole web), list pipelines, list its project's apps, start and run its own site's works.
/// `apps.list` is answered for its own project, whatever it asked.
pub fn app_may(grant: &AppGrant, command: &mut ClientCommand) -> Result<(), String> {
    let own = |site: &str| site == grant.site;
    let allowed = match command {
        ClientCommand::Subscribe { .. } | ClientCommand::PipelineList => true,
        ClientCommand::AppsList { project } => {
            *project = grant.project.as_ref().map(|p| p.name.to_string());
            true
        }
        ClientCommand::WorkNew { pipeline, .. } => own(pipeline),
        ClientCommand::WorkRun { work, .. } => own(work_site(work)),
        _ => false,
    };
    if allowed {
        return Ok(());
    }
    let name = serde_json::to_value(&*command)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
        .unwrap_or_else(|| "that".into());
    Err(format!(
        "the {} app's token can't send `{name}`: an app's socket may `subscribe`, \
         `pipeline.list`, `apps.list`, and `work.new`/`work.run` for site://{}'s works",
        grant.site, grant.site
    ))
}

/// The site of a work (`site://cad/arm`, `cad/arm`).
fn work_site(work: &str) -> &str {
    let work = work.strip_prefix("site://").unwrap_or(work);
    work.split('/').next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::ProjectName;

    fn grant() -> AppGrant {
        AppGrant {
            project: Some(crate::apps::GrantedProject {
                name: ProjectName::new("game").unwrap(),
                folder: "/p/game".into(),
            }),
            site: "cad".into(),
            local: None,
            session: None,
        }
    }

    #[test]
    fn only_what_an_app_needs() {
        let run = |work: &str| ClientCommand::WorkRun {
            work: work.into(),
            stage: None,
            session: None,
        };
        let mut ok = [
            ClientCommand::Subscribe { urls: vec![] },
            ClientCommand::PipelineList,
            run("site://cad/arm"),
            run("cad/arm"),
        ];
        for command in &mut ok {
            assert!(app_may(&grant(), command).is_ok(), "{command:?}");
        }
        let mut list = ClientCommand::AppsList { project: None };
        app_may(&grant(), &mut list).unwrap();
        assert_eq!(
            list,
            ClientCommand::AppsList {
                project: Some("game".into())
            }
        );
        let mut refused = [
            run("site://video/teaser"),
            ClientCommand::SessionList,
            ClientCommand::McpList,
        ];
        for command in &mut refused {
            let error = app_may(&grant(), command).unwrap_err();
            assert!(error.contains("can't send"), "{error}");
        }
    }
}
