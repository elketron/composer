//! New workspaces from templates (ticket 0154): the stock sites, the template's library sites and
//! its config; refused before anything is written when the template or the name is wrong; and a
//! running host serves a new workspace right away.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use composer_server::library::{Library, SiteName, WorkspaceLibrary};
use composer_server::{
    Home, Host, Server, Starter, TemplateName, WorkspaceName, create_workspace, init_home,
};
use serde_json::{Value, json};
use support::MockProvider;

/// A home with a library site `cad` and a template `3d` installing it, with a config overlay.
async fn home() -> (tempfile::TempDir, Home) {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let cad = home.library().join("cad");
    std::fs::create_dir_all(cad.join("stages")).unwrap();
    std::fs::write(cad.join("index.md"), "---\ntitle: CAD\n---\n# CAD\n").unwrap();
    std::fs::write(cad.join("stages/01_plan.md"), "# Plan\n").unwrap();
    Library::of(&home).record("publish: cad").unwrap();
    let templates = home.global().join("workspace-templates");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(
        templates.join("3d.toml"),
        "description = \"3D printing\"\nsites = [\"cad\"]\n\n[config.agent]\nslots = 2\n",
    )
    .unwrap();
    std::fs::write(
        templates.join("broken.toml"),
        "sites = [\"cad\", \"nope\"]\n",
    )
    .unwrap();
    std::fs::write(
        templates.join("badconfig.toml"),
        "[config.agent]\nslots = \"many\"\n",
    )
    .unwrap();
    (dir, home)
}

fn name(n: &str) -> WorkspaceName {
    WorkspaceName::new(n).unwrap()
}

#[tokio::test]
async fn a_workspace_from_a_template_has_its_sites_and_config() {
    let (_dir, home) = home().await;
    let workspace = create_workspace(
        &home,
        &name("printing"),
        Some(&TemplateName::new("3d").unwrap()),
    )
    .await
    .unwrap();
    assert!(workspace.sites().join("cad/stages/01_plan.md").is_file());
    // The stock sites too, agents included, all installed from the library.
    assert!(workspace.sites().join("agents/default.md").is_file());
    let installed = WorkspaceLibrary::of(&workspace)
        .unwrap()
        .installed()
        .unwrap();
    for site in ["cad", "agents", "orchestration"] {
        assert!(
            installed.get(&SiteName::new(site).unwrap()).is_some(),
            "{site}"
        );
    }
    assert_eq!(workspace.load_config().unwrap().agent.slots, 2);
    let config = std::fs::read_to_string(workspace.config_file()).unwrap();
    assert!(
        config.contains("# From workspace template `3d`."),
        "{config}"
    );
    // Listed among the templates.
    let all = composer_server::WorkspaceTemplate::all(&home);
    assert!(all.contains(&("3d".to_string(), "3D printing".to_string())));
}

#[tokio::test]
async fn a_wrong_template_or_name_writes_nothing() {
    let (_dir, home) = home().await;
    let error = create_workspace(
        &home,
        &name("x"),
        Some(&TemplateName::new("broken").unwrap()),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("nope") && !error.contains("cad,"), "{error}");
    assert!(!home.workspace(&name("x")).root().exists());

    let error = create_workspace(
        &home,
        &name("x"),
        Some(&TemplateName::new("missing").unwrap()),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("no workspace template `missing`"), "{error}");

    // A config the template breaks: the workspace made so far is removed again.
    let error = create_workspace(
        &home,
        &name("x"),
        Some(&TemplateName::new("badconfig").unwrap()),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("badconfig"), "{error}");
    assert!(!home.workspace(&name("x")).root().exists());

    let error = create_workspace(&home, &name("default"), None)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("exists already"), "{error}");
    assert!(WorkspaceName::new("global").is_err(), "reserved");
}

#[tokio::test]
async fn a_running_host_serves_a_new_workspace_right_away() {
    let (_dir, home) = home().await;
    let mut servers = BTreeMap::new();
    let default = home.workspace(&WorkspaceName::default_workspace());
    let config = default.load_config().unwrap();
    let provider = Arc::new(MockProvider::new(vec![]));
    servers.insert(
        WorkspaceName::default_workspace(),
        Server::new(default, config, provider).unwrap(),
    );
    let host = Arc::new(Host::new(home.clone(), servers).unwrap());
    let starter: Starter = Arc::new(|workspace| {
        Box::pin(async move {
            let config = workspace.load_config()?;
            Server::new(workspace, config, Arc::new(MockProvider::new(vec![])))
        })
    });
    host.set_starter(starter);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = host.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    while host.default_workspace().parent_url().is_none() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let master = std::fs::read_to_string(home.master_token_file()).unwrap();
    let client = reqwest::Client::new();
    let create = |token: String, body: Value| {
        let client = client.clone();
        async move {
            let response = client
                .post(format!("http://{addr}/api/workspaces"))
                .bearer_auth(token)
                .json(&body)
                .send()
                .await
                .unwrap();
            (response.status().as_u16(), response.text().await.unwrap())
        }
    };

    // Only the master token creates.
    let default_token = host.default_workspace().token.clone();
    let (status, _) = create(default_token, json!({ "name": "printing" })).await;
    assert_eq!(status, 404);
    assert!(!home.workspace(&name("printing")).root().exists());

    let (status, text) = create(
        master.trim().to_string(),
        json!({ "name": "printing", "template": "3d" }),
    )
    .await;
    assert_eq!(status, 201, "{text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["base"], "/w/printing");

    // Served at once, with its own token; the default's token doesn't open it.
    let printing = host.workspace(&name("printing")).unwrap();
    let get = |path: String, token: String| {
        let client = client.clone();
        async move {
            client
                .get(format!("http://{addr}{path}"))
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }
    };
    assert_eq!(
        get("/w/printing/api/pages/cad".into(), printing.token.clone()).await,
        200
    );
    assert_eq!(
        get(
            "/w/printing/api/info".into(),
            host.default_workspace().token.clone()
        )
        .await,
        404
    );
    let ws = format!("ws://{addr}/w/printing/ws?token={}", printing.token);
    assert!(tokio_tungstenite::connect_async(ws).await.is_ok());
    // The list knows it.
    let response = client
        .get(format!("http://{addr}/api/workspaces"))
        .bearer_auth(master.trim())
        .send()
        .await
        .unwrap();
    let list: Value = response.json().await.unwrap();
    assert_eq!(list.as_array().unwrap().len(), 2, "{list}");

    // Again: it exists.
    let (status, text) = create(master.trim().to_string(), json!({ "name": "printing" })).await;
    assert_eq!(status, 409, "{text}");
}
