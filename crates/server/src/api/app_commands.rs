use composer_protocol::ClientCommand;

use crate::apps::AppGrant;

/// Whether an app's socket may send `command`: subscribe to pages (it reads the whole web) and
/// list its project's apps. `apps.list` is answered for its own project, whatever it asked.
pub fn app_may(grant: &AppGrant, command: &mut ClientCommand) -> Result<(), String> {
    let allowed = match command {
        ClientCommand::Subscribe { .. } => true,
        ClientCommand::AppsList { project } => {
            *project = grant.project.as_ref().map(|p| p.name.to_string());
            true
        }
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
        "the {} app's token can't send `{name}`: an app's socket may `subscribe` and `apps.list`",
        grant.site
    ))
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
        let mut subscribe = ClientCommand::Subscribe { urls: vec![] };
        assert!(app_may(&grant(), &mut subscribe).is_ok());
        let mut list = ClientCommand::AppsList { project: None };
        app_may(&grant(), &mut list).unwrap();
        assert_eq!(
            list,
            ClientCommand::AppsList {
                project: Some("game".into())
            }
        );
        let mut other = ClientCommand::ProjectsOpen;
        assert!(app_may(&grant(), &mut other).is_err());
    }
}
