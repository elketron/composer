//! The launcher over HTTP: config `[[launcher]]` entries listed to apps, launched by name,
//! and scope refusals. The launched processes are stubs that touch a marker and exit.

mod support;

use composer_server::{Config, LauncherEntry};
use support::*;

async fn http(
    server: &composer_server::Server,
    addr: std::net::SocketAddr,
    path: &str,
) -> (u16, serde_json::Value) {
    let response = reqwest::Client::new()
        .post(format!("http://{addr}{path}"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    (
        response.status().as_u16(),
        response.json().await.unwrap_or_default(),
    )
}

async fn get(
    server: &composer_server::Server,
    addr: std::net::SocketAddr,
    path: &str,
) -> (u16, serde_json::Value) {
    let response = reqwest::Client::new()
        .get(format!("http://{addr}{path}"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    (
        response.status().as_u16(),
        response.json().await.unwrap_or_default(),
    )
}

fn entry(name: &str, command: &str, folder: &str, args: &[&str]) -> LauncherEntry {
    LauncherEntry {
        name: name.into(),
        command: command.into(),
        folder: folder.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
    }
}

/// A stub that waits a moment, writes its marker into its working directory (the entry's
/// folder), and exits.
fn stub(name: &str, folder: &str) -> LauncherEntry {
    entry(
        name,
        "/bin/sh",
        folder,
        &["-c", "sleep 0.3; touch marker", "x"],
    )
}

async fn wait_for(marker: &std::path::Path) -> bool {
    for _ in 0..100 {
        if marker.exists() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn routes_need_the_token() {
    let (_dir, _server, _p, addr) = start(vec![]).await;
    let client = reqwest::Client::new();
    let list = client
        .get(format!("http://{addr}/api/launcher"))
        .send()
        .await
        .unwrap();
    let launch = client
        .post(format!("http://{addr}/api/launcher/anything"))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 401);
    assert_eq!(launch.status(), 401);
}

#[tokio::test]
async fn an_app_lists_entries_and_launches_one_by_name() {
    let projects = tempfile::tempdir().unwrap();
    let slice = projects.path().join("app/blockouts");
    std::fs::create_dir_all(&slice).unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().to_str().unwrap().into();
    config.launcher = vec![stub("stub", "app/blockouts")];
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;

    let (status, list) = get(&server, addr, "/api/launcher").await;
    assert_eq!(status, 200);
    assert_eq!(list[0]["name"], "stub");
    assert_eq!(list[0]["command"], "/bin/sh");

    let (status, result) = http(&server, addr, "/api/launcher/stub").await;
    assert_eq!(status, 200, "{}", result);
    assert_eq!(result["status"], "launched");
    assert!(result["pid"].as_u64().unwrap() > 0);
    assert!(wait_for(&slice.join("marker")).await, "the stub never ran");
}

#[tokio::test]
async fn an_entry_outside_the_allowed_scope_refuses_to_launch() {
    let projects = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().to_str().unwrap().into();
    config.launcher = vec![stub("stub", outside.path().to_str().unwrap())];
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;

    let (status, body) = http(&server, addr, "/api/launcher/stub").await;
    assert_eq!(status, 403, "{}", body);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("outside the allowed scope"),
        "{body}"
    );
    // It refused rather than ran: no marker.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(!outside.path().join("marker").exists());
}

#[tokio::test]
async fn an_unknown_entry_is_404() {
    let (_dir, server, _p, addr) = start(vec![]).await;
    let (status, _) = http(&server, addr, "/api/launcher/nope").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn the_folder_is_the_last_argument() {
    let projects = tempfile::tempdir().unwrap();
    let slice = projects.path().join("app/blockouts");
    std::fs::create_dir_all(&slice).unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().to_str().unwrap().into();
    // Leaves the working directory first, so only "$1" (the folder argument) finds the slice.
    config.launcher = vec![entry(
        "stub",
        "/bin/sh",
        "app/blockouts",
        &["-c", "cd /; touch \"$1/marker\"", "x"],
    )];
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;
    let (status, result) = http(&server, addr, "/api/launcher/stub").await;
    assert_eq!(status, 200, "{}", result);
    assert!(
        wait_for(&slice.join("marker")).await,
        "the folder was not passed last"
    );
}

#[tokio::test]
async fn a_symlinked_projects_directory_still_launches() {
    let real = tempfile::tempdir().unwrap();
    let links = tempfile::tempdir().unwrap();
    let projects = links.path().join("Projects");
    std::os::unix::fs::symlink(real.path(), &projects).unwrap();
    std::fs::create_dir_all(real.path().join("app")).unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.to_str().unwrap().into();
    config.launcher = vec![stub("stub", "app")];
    let (_dir, server, _p, addr) = start_with(config, vec![]).await;
    let (status, result) = http(&server, addr, "/api/launcher/stub").await;
    assert_eq!(status, 200, "{}", result);
}
