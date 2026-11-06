//! Two workspaces in one host (tickets 0145, 0146): each has its own web, index, token, sessions
//! and state; its API lives only at `/w/<name>/`, and a token opens only its own workspace.

mod support;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_server::{Home, Host, Server, WorkspaceName, init_home, init_workspace};
use serde_json::{Value, json};
use support::MockProvider;

/// A home with `default` and `b`, served by one host on a random port.
async fn two_workspaces() -> (tempfile::TempDir, Arc<Host>, SocketAddr) {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let b = WorkspaceName::new("b").unwrap();
    init_workspace(&home.workspace(&b)).await.unwrap();
    let mut servers = BTreeMap::new();
    for name in home.workspace_names().unwrap() {
        let workspace = home.workspace(&name);
        let config = workspace.load_config().unwrap();
        let provider = Arc::new(MockProvider::new(vec![]));
        servers.insert(name, Server::new(workspace, config, provider).unwrap());
    }
    let host = Arc::new(Host::new(home, servers).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = host.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    // `serve` mounts the routes in the background; wait until b's answer.
    let b = host.workspace(&b).unwrap().clone();
    for _ in 0..100 {
        if b.parent_url().is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    (dir, host, addr)
}

fn server<'a>(host: &'a Host, name: &str) -> &'a Arc<Server> {
    host.workspace(&WorkspaceName::new(name).unwrap()).unwrap()
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// GET `path` with `token`: the status and, when it is JSON, the body.
async fn get(addr: SocketAddr, path: &str, token: &str) -> (u16, Value) {
    let response = client()
        .get(format!("http://{addr}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn hits(body: &Value) -> usize {
    body.as_array().map_or(0, Vec::len)
}

fn names(body: &Value) -> Vec<String> {
    body.as_array()
        .into_iter()
        .flatten()
        .map(|w| w["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn each_workspace_has_its_own_web_index_and_token() {
    let (_dir, host, addr) = two_workspaces().await;
    let (a, b) = (server(&host, "default"), server(&host, "b"));

    assert_ne!(a.token, b.token, "each workspace has its own token");
    assert_ne!(a.web.root(), b.web.root());
    assert_ne!(a.home.sessions(), b.home.sessions());
    assert_ne!(a.home.approvals_file(), b.home.approvals_file());
    assert_eq!(
        a.home.templates(),
        b.home.templates(),
        "templates are shared"
    );

    let url = SiteUrl::parse("site://memory/zebra-note.md").unwrap();
    a.write_page(
        &url,
        "---\ntitle: Zebra note\n---\n# Zebra note\n",
        ChangedBy::App,
    )
    .unwrap();
    assert!(a.web.root().join("memory/zebra-note.md").is_file());
    assert!(!b.web.root().join("memory/zebra-note.md").exists());

    // Each workspace's API is under /w/<name>/ and searches only its own index.
    let (status, found) = get(addr, "/w/default/api/search?q=zebra", &a.token).await;
    assert_eq!(status, 200);
    assert_eq!(hits(&found), 1, "{found}");
    let (status, found) = get(addr, "/w/b/api/search?q=zebra", &b.token).await;
    assert_eq!(status, 200);
    assert_eq!(hits(&found), 0, "{found}");
}

#[tokio::test]
async fn a_token_opens_only_its_own_workspace_and_others_look_absent() {
    let (_dir, host, addr) = two_workspaces().await;
    let (a, b) = (server(&host, "default"), server(&host, "b"));

    for (path, token) in [
        ("/w/b/api/info", &a.token),
        ("/w/default/api/info", &b.token),
        ("/w/b/api/search?q=", &a.token),
        ("/w/b/chat/", &a.token),
    ] {
        let (status, _) = get(addr, path, token).await;
        assert_eq!(status, 404, "{path}: another workspace's token");
    }
    // A wrong token looks exactly like a workspace that doesn't exist.
    let (status, _) = get(addr, "/w/b/api/info", "nope").await;
    assert_eq!(status, 404);
    let (status, _) = get(addr, "/w/nope/api/info", &a.token).await;
    assert_eq!(status, 404);
    // No routes outside /w/<name>/ but the workspace list.
    for path in ["/api/info", "/ws", "/chat/", "/pair"] {
        let (status, _) = get(addr, path, &a.token).await;
        assert_eq!(status, 404, "{path}");
    }
    let (status, _) = get(addr, "/w/b/api/info", &b.token).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn the_workspace_list_shows_what_a_token_opens() {
    let (_dir, host, addr) = two_workspaces().await;
    let (a, b) = (server(&host, "default"), server(&host, "b"));
    let master = std::fs::read_to_string(host.home().master_token_file()).unwrap();

    let (status, all) = get(addr, "/api/workspaces", master.trim()).await;
    assert_eq!(status, 200);
    assert_eq!(names(&all), ["b", "default"]);
    assert_eq!(all[0]["base"], "/w/b");
    let (_, only_a) = get(addr, "/api/workspaces", &a.token).await;
    assert_eq!(names(&only_a), ["default"]);
    let (_, only_b) = get(addr, "/api/workspaces", &b.token).await;
    assert_eq!(names(&only_b), ["b"]);
    let (status, _) = get(addr, "/api/workspaces", "nope").await;
    assert_eq!(status, 404);
    // The master token lists workspaces; it opens none of them.
    let (status, _) = get(addr, "/w/b/api/info", master.trim()).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_device_paired_to_one_workspace_opens_only_that_one() {
    let (_dir, host, addr) = two_workspaces().await;
    let b = server(&host, "b");
    let code = b.devices.create_code("phone").unwrap();

    // Pairing a's routes with b's code fails: codes are per workspace too.
    let response = client()
        .post(format!("http://{addr}/w/default/api/pair"))
        .json(&json!({ "code": code, "name": "phone" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 401);

    let response = client()
        .post(format!("http://{addr}/w/b/api/pair"))
        .json(&json!({ "code": code, "name": "phone" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let paired: Value = response.json().await.unwrap();
    let token = paired["token"].as_str().unwrap();

    let (status, _) = get(addr, "/w/b/api/info", token).await;
    assert_eq!(status, 200);
    let (status, _) = get(addr, "/w/default/api/info", token).await;
    assert_eq!(status, 404);
    let (_, listed) = get(addr, "/api/workspaces", token).await;
    assert_eq!(names(&listed), ["b"]);
}

#[tokio::test]
async fn browsers_keep_one_cookie_per_workspace_and_stay_inside_it() {
    let (_dir, host, addr) = two_workspaces().await;
    let b = server(&host, "b");

    let response = client()
        .get(format!("http://{addr}/w/b/api/info?token={}", b.token))
        .send()
        .await
        .unwrap();
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.contains("Path=/w/b/;"), "{cookie}");

    // `/w/b/chat` → `chat/`, relative, so the browser stays in b.
    let response = client()
        .get(format!("http://{addr}/w/b/chat?token={}", b.token))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 308);
    assert_eq!(
        response.headers()["location"],
        "chat/?token=".to_string() + &b.token
    );

    // The pairing form posts to its own workspace.
    let response = client()
        .get(format!("http://{addr}/w/b/pair?code=x"))
        .send()
        .await
        .unwrap();
    let page = response.text().await.unwrap();
    assert!(page.contains("action=\"pair\""), "{page}");
}

#[tokio::test]
async fn workers_are_pointed_at_their_own_workspace() {
    let (_dir, host, addr) = two_workspaces().await;
    let b = server(&host, "b");
    assert_eq!(b.parent_url().unwrap(), format!("http://{addr}/w/b"));
    assert_eq!(
        server(&host, "default").parent_url().unwrap(),
        format!("http://{addr}/w/default")
    );
    // A child process started from b's environment reopens b, not the home's default.
    let env: BTreeMap<_, _> = b.home.env().into_iter().collect();
    assert_eq!(env["COMPOSER_WORKSPACE"], "b");
    assert_eq!(
        env["COMPOSER_WORKSPACE_HOME"],
        b.home.root().display().to_string()
    );
}
