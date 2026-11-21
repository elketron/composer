//! What the app lists: sites with their descriptions.

mod support;

use composer_server::Config;
use support::*;

#[tokio::test]
async fn sites() {
    let (_dir, server, _p, addr) = start_with(Config::defaults(), vec![]).await;
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
