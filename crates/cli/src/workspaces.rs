//! `composer workspace new|list|templates`.

use composer_server::library::WorkspaceLibrary;
use composer_server::{Config, Home, WorkspaceName, WorkspaceTemplate, create_workspace};

/// Creates a workspace. With a server running, the server makes it and serves it right away;
/// otherwise it is made on disk and served from the next start.
pub async fn new(home: &Home, name: &str, template: Option<String>) -> anyhow::Result<()> {
    let name = WorkspaceName::new(name)?;
    let bind = Config::load(&home.global_config_file())?.server.bind;
    if std::net::TcpStream::connect(&bind).is_ok() {
        let master = std::fs::read_to_string(home.master_token_file())?;
        let response = reqwest::Client::new()
            .post(format!("http://{bind}/api/workspaces"))
            .bearer_auth(master.trim())
            .json(&serde_json::json!({ "name": name.as_str(), "template": template }))
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            anyhow::bail!("the server on {bind} didn't create it: {text}");
        }
        println!("workspace {name} created and served at http://{bind}/w/{name}/");
    } else {
        let workspace = create_workspace(home, &name, template.as_deref()).await?;
        println!(
            "workspace {name} created at {} (served from the next `composer serve`)",
            workspace.root().display()
        );
    }
    if let Some(template) = template {
        println!("from template {template}");
    }
    Ok(())
}

/// Every workspace, with the library sites it installed (and how many the library changed).
pub fn list(home: &Home) -> anyhow::Result<()> {
    for name in home.workspace_names()? {
        let workspace = home.workspace(&name);
        let Ok(library) = WorkspaceLibrary::of(&workspace) else {
            println!("{}", name.as_str());
            continue;
        };
        let installed: Vec<String> = library
            .installed()
            .map(|sites| sites.iter().map(|(site, _)| site.to_string()).collect())
            .unwrap_or_default();
        let behind = library.behind();
        let behind = match behind {
            0 => String::new(),
            n => format!(" ({n} with library updates)"),
        };
        println!(
            "{:<16} from the library: {}{behind}",
            name.as_str(),
            installed.join(", ")
        );
    }
    Ok(())
}

/// The workspace templates in `global/workspace-templates/`.
pub fn templates(home: &Home) -> anyhow::Result<()> {
    let all = WorkspaceTemplate::all(home);
    if all.is_empty() {
        println!(
            "no workspace templates yet: add one as {}/<name>.toml",
            WorkspaceTemplate::folder(home).display()
        );
    }
    for (name, description) in all {
        println!("{name:<16} {description}");
    }
    Ok(())
}
