//! What the app lists: agents, projects, and sites with their descriptions.

mod support;

use composer_server::Config;
use support::*;

#[tokio::test]
async fn agents_projects_and_sites() {
    let projects = tempfile::tempdir().unwrap();
    for name in ["older", ".hidden", "newer"] {
        std::fs::create_dir(projects.path().join(name)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    std::fs::write(projects.path().join("file.txt"), "").unwrap();
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
    assert_eq!(agent("default")["worker"], false);
    assert_eq!(agent("orchestrator")["worker"], false);
    assert_eq!(agent("worker")["worker"], true);
    assert_eq!(
        agent("worker")["description"],
        "general worker; carries out any ticket"
    );

    let listed: Vec<String> = get("/api/projects")
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        listed,
        ["newer", "older"],
        "newest first, folders only, no hidden"
    );

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
