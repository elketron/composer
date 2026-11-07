use crate::error::{Result, ServerError};
use crate::home::Home;
use crate::init::init_workspace;
use crate::library::Library;
use crate::template_name::TemplateName;
use crate::workspace_home::WorkspaceHome;
use crate::workspace_name::WorkspaceName;
use crate::workspace_template::WorkspaceTemplate;

/// Makes workspace `name` of `home` on disk: its stock sites (from the library), then the
/// template's sites and config. Refused when it exists or the template names a site the library
/// doesn't have (before anything is written); a later failure removes what was made.
pub async fn create_workspace(
    home: &Home,
    name: &WorkspaceName,
    template: Option<&TemplateName>,
) -> Result<WorkspaceHome> {
    let workspace = home.workspace(name);
    if workspace.root().exists() {
        return Err(ServerError::Invalid(format!(
            "workspace {name} exists already; pick another name"
        )));
    }
    let template = template
        .map(|t| WorkspaceTemplate::load(home, t))
        .transpose()
        .map_err(ServerError::Invalid)?;
    if let Some(template) = &template {
        template
            .check(&Library::of(home))
            .map_err(ServerError::Invalid)?;
    }
    let made = async {
        init_workspace(&workspace).await?;
        if let Some(template) = &template {
            template
                .apply(&workspace)
                .await
                .map_err(ServerError::Invalid)?;
        }
        Ok(())
    }
    .await;
    if let Err(e) = made {
        let _ = std::fs::remove_dir_all(workspace.root());
        return Err(e);
    }
    Ok(workspace)
}
