//! Two workspaces in one host (ticket 0145): each has its own web, index, token, sessions and
//! state, and its API is mounted at `/w/<name>/`.

mod support;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_server::{Home, Host, Server, WorkspaceName, init_home, init_workspace};
use serde_json::Value;
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
    let host = Arc::new(Host::new(home, servers));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = host.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    (dir, host, addr)
}

fn server<'a>(host: &'a Host, name: &str) -> &'a Arc<Server> {
    host.workspace(&WorkspaceName::new(name).unwrap()).unwrap()
}

/// GET `path` with `token`: the status and, when it is JSON, the body.
async fn get(addr: SocketAddr, path: &str, token: &str) -> (u16, Value) {
    let response = reqwest::Client::new()
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

    // Each workspace's API is mounted under /w/<name>/ and searches only its own index.
    let (status, found) = get(addr, "/w/default/api/search?q=zebra", &a.token).await;
    assert_eq!(status, 200);
    assert_eq!(hits(&found), 1, "{found}");
    let (status, found) = get(addr, "/w/b/api/search?q=zebra", &b.token).await;
    assert_eq!(status, 200);
    assert_eq!(hits(&found), 0, "{found}");

    // A workspace's token opens only that workspace.
    let (status, _) = get(addr, "/w/b/api/search?q=zebra", &a.token).await;
    assert_eq!(status, 401, "a's token on b");
    let (status, _) = get(addr, "/w/default/api/search?q=zebra", &b.token).await;
    assert_eq!(status, 401, "b's token on default");

    // Transitional (until 0146): unprefixed routes serve the default workspace.
    let (status, found) = get(addr, "/api/search?q=zebra", &a.token).await;
    assert_eq!(status, 200);
    assert_eq!(hits(&found), 1);
    let (status, _) = get(addr, "/api/search?q=zebra", &b.token).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn workers_are_pointed_at_their_own_workspace() {
    let (_dir, host, addr) = two_workspaces().await;
    // `serve` sets the listen address in the background; wait for it.
    let b = server(&host, "b");
    for _ in 0..50 {
        if b.parent_url().is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
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
