//! What the app lists: agents, and sites with their descriptions.

mod support;

use composer_server::Config;
use support::*;

#[tokio::test]
async fn agents_and_sites() {
    let projects = tempfile::tempdir().unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;
    let client = reqwest::Client::new();
    let get = |path: &str| {
        let request = client
            .get(format!("http://{addr}{path}"))
            .bearer_auth(&server.token);
        async move {
            request
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap()
        }
    };

    let info = get("/api/info").await;
    assert_eq!(info["projects_dir"], projects.path().display().to_string());

    let agents = get("/api/agents").await;
    let agent = |name: &str| {
        agents
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .cloned()
            .unwrap()
    };
    // The built-in agent, without a page.
    assert_eq!(agent("default")["title"], "Composer");

    let sites = get("/api/sites").await;
    let memory = sites
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "memory")
        .unwrap();
    assert!(memory["pages"].as_u64().unwrap() > 1, "{memory}");
    assert!(
        !memory["description"].as_str().unwrap().is_empty(),
        "{memory}"
    );
}
