//! An app on its project (ticket 0164): the sdk's `project()` (`<app>/api/project`), its work data
//! for the project (`<app>/api/data/...`, in the workspace's state, not the project), and its
//! recipes, which get `PROJECT_ROOT`, `PROJECT_NAME` and `APP_DATA` and can write the project and
//! the data, not another project. An ignored test builds a fresh app from the template in the
//! sandbox and opens it on a project (needs `bun` and its package cache).
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_server::Config;
use composer_server::actions::{Caller, Justfile};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use support::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn has(tool: &str) -> bool {
    std::process::Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

async fn call(
    addr: SocketAddr,
    method: Method,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> (u16, String) {
    let mut request = Client::new()
        .request(method, format!("http://{addr}{path}"))
        .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

/// A projects folder with `game` (git, on `main`, tagged, linked by a cad work) and `notes`; a
/// server whose `cad` app attaches to `game`.
async fn setup() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    tempfile::TempDir,
    std::sync::Arc<composer_server::Server>,
    SocketAddr,
) {
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    let game = root.join("game");
    write(&game.join("arm.FCStd"), "");
    git(&game, &["init", "-q", "-b", "main"]);
    git(&game, &["add", "."]);
    git(&game, &["commit", "-qm", "init"]);
    write(&root.join("notes/todo.txt"), "");
    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    config.projects.settings.insert(
        game.display().to_string(),
        composer_server::ProjectSettings {
            tags: vec!["hardware".into()],
            ..Default::default()
        },
    );
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let cad = dir.path().join("sites/cad");
    write(
        &cad.join("index.md"),
        "---\ntitle: CAD\ntype: index\napp:\n  scope: project\n  when: [\"files:*.FCStd\"]\n---\n# CAD\n",
    );
    write(
        &cad.join("arm/index.md"),
        &format!(
            "---\ntitle: Arm\ntype: work\nproject: {}\n---\n# Arm\n",
            game.display()
        ),
    );
    (projects, root, dir, server, addr)
}

/// Opens `site` on `game`: the app's token.
async fn open(server: &composer_server::Server, addr: SocketAddr, site: &str) -> String {
    let (status, body) = call(
        addr,
        Method::POST,
        &format!("/api/projects/game/apps/{site}/open"),
        &server.token,
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str::<Value>(&body).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn an_app_knows_its_project_and_keeps_its_data_in_the_workspace() {
    let (_projects, root, dir, server, addr) = setup().await;
    let token = open(&server, addr, "cad").await;

    let (status, body) = call(
        addr,
        Method::GET,
        &format!("/a/{token}/api/project"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let info: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        info,
        json!({
            "name": "game",
            "root": root.join("game").display().to_string(),
            "branch": "main",
            "sites": [],
            "tags": ["hardware"],
        })
    );

    // Data: round-trips, lives in the workspace's state, not in the project.
    let (status, body) = call(
        addr,
        Method::PUT,
        &format!("/a/{token}/api/data/views/arm.json"),
        &token,
        Some(json!({ "zoom": 2 })),
    )
    .await;
    assert_eq!(status, 204, "{body}");
    let stored = dir.path().join("state/projects/game/cad/views/arm.json");
    assert!(stored.is_file());
    assert!(!root.join("game/views").exists());
    let (status, body) = call(
        addr,
        Method::GET,
        &format!("/a/{token}/api/data/views/arm.json"),
        &token,
        None,
    )
    .await;
    assert_eq!((status, body.as_str()), (200, r#"{"zoom":2}"#));
    let (status, _) = call(
        addr,
        Method::GET,
        &format!("/a/{token}/api/data/nope.json"),
        &token,
        None,
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn an_apps_recipe_gets_its_project_and_data() {
    if !has("just") {
        return;
    }
    let (_projects, root, dir, server, addr) = setup().await;
    let cad = dir.path().join("sites/cad");
    let notes = root.join("notes");
    write(
        &cad.join("justfile"),
        &format!(
            "probe:\n    echo \"$PROJECT_NAME|$PROJECT_ROOT\" > \"$APP_DATA/env.txt\"\n    touch \"$PROJECT_ROOT/from-recipe\"\n    touch '{}/escaped' 2>/dev/null || true\n",
            notes.display()
        ),
    );
    let justfile = Justfile::load(&cad).await.unwrap();
    server
        .approvals
        .approve("cad", "probe", &justfile.hash("probe"), Caller::App)
        .unwrap();
    let token = open(&server, addr, "cad").await;

    let (status, body) = call(
        addr,
        Method::POST,
        &format!("/a/{token}/api/actions/cad/probe"),
        &token,
        Some(json!({ "args": [] })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let result: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    let env = std::fs::read_to_string(dir.path().join("state/projects/game/cad/env.txt")).unwrap();
    assert_eq!(env.trim(), format!("game|{}", root.join("game").display()));
    assert!(root.join("game/from-recipe").is_file());
    assert!(!notes.join("escaped").exists(), "another project");
}

/// `cargo test -p composer-server --test app_project -- --ignored`: a fresh app from the
/// template builds with its `ui-build` in the sandbox and runs on a project.
#[tokio::test]
#[ignore = "needs bun and its package cache; run on demand"]
async fn a_fresh_app_from_the_template_builds_and_runs_on_a_project() {
    let (_projects, _root, dir, server, addr) = setup().await;
    let ui = dir.path().join("sites/cad/ui");
    copy_template(&server.home.templates().join("app"), &ui, "cad");
    write(
        &dir.path().join("sites/cad/justfile"),
        "ui-build:\n    cd ui && bun install && bun run build\n",
    );
    let justfile = Justfile::load(&dir.path().join("sites/cad")).await.unwrap();
    server
        .approvals
        .approve("cad", "ui-build", &justfile.hash("ui-build"), Caller::App)
        .unwrap();
    let token = open(&server, addr, "cad").await;
    let (status, body) = call(
        addr,
        Method::POST,
        &format!("/a/{token}/api/actions/cad/ui-build"),
        &token,
        Some(json!({ "args": [] })),
    )
    .await;
    let result: Value = serde_json::from_str(&body).unwrap_or_default();
    assert_eq!(
        (status, result["status"].as_str()),
        (200, Some("ok")),
        "{body}"
    );
    let (status, page) = call(addr, Method::GET, &format!("/a/{token}/"), &token, None).await;
    assert_eq!(status, 200);
    assert!(page.contains("<script"), "{page}");
}

fn copy_template(from: &Path, to: &Path, site: &str) {
    for entry in walk(from) {
        let target = to.join(entry.strip_prefix(from).unwrap());
        let text = std::fs::read_to_string(&entry).unwrap();
        write(&target, &text.replace("__SITE__", site));
    }
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
