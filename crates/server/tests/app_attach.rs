//! Apps attached to projects (ticket 0162): a site's `app:` block says where it shows and which
//! projects it attaches to; a project's `[projects."<path>"]` entry pins, hides, tags and names.
//! Listed over HTTP (`/api/apps`, `/api/projects/<name>/apps`) and ws (`apps.list`).
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::{Config, ProjectSettings};
use serde_json::Value;
use support::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

async fn get(addr: SocketAddr, token: &str, path: &str) -> (u16, Value) {
    let response = reqwest::Client::new()
        .get(format!("http://{addr}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    (
        response.status().as_u16(),
        response.json().await.unwrap_or_default(),
    )
}

/// `(site, why)` of a listing.
fn apps(list: &Value) -> Vec<(String, String)> {
    list["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["site"].as_str().unwrap().to_string(),
                a["why"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn projects_get_the_apps_that_match_them() {
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    // `game`: a git repo with a FreeCAD file; `notes`: a plain folder a video work links.
    let game = root.join("game");
    write(&game.join("parts/arm.FCStd"), "");
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&game)
        .status()
        .unwrap();
    let notes = root.join("notes");
    write(&notes.join("todo.txt"), "");

    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    config.projects.settings.insert(
        game.display().to_string(),
        ProjectSettings {
            name: Some("mygame".into()),
            hide_apps: vec!["board".into()],
            ..Default::default()
        },
    );
    config.projects.settings.insert(
        notes.display().to_string(),
        ProjectSettings {
            apps: vec!["calendar".into()],
            tags: vec!["writing".into()],
            ..Default::default()
        },
    );
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let sites = dir.path().join("sites");
    write(
        &sites.join("cad/index.md"),
        "---\ntitle: CAD\napp:\n  scope: project\n  when: [\"files:*.FCStd\"]\n  rail: { icon: cube, label: Parts }\n  opens: [\"*.FCStd\"]\n---\n# CAD\n",
    );
    write(
        &sites.join("board/index.md"),
        "---\ntitle: Board\napp:\n  scope: both\n  when: [git, \"tag:writing\"]\n  tile: true\n---\n# Board\n",
    );
    write(
        &sites.join("calendar/index.md"),
        "---\ntitle: Calendar\n---\n# Calendar\n",
    );
    std::fs::create_dir_all(sites.join("calendar/ui")).unwrap();
    write(
        &sites.join("broken/index.md"),
        "---\ntitle: Broken\napp:\n  scope: everywhere\n---\n# Broken\n",
    );
    let token = server.token.clone();

    // `game` is called `mygame`; board matches (git) but is hidden there.
    let (status, game_apps) = get(addr, &token, "/api/projects/mygame/apps").await;
    assert_eq!(status, 200, "{game_apps}");
    assert_eq!(game_apps["project"], "mygame");
    assert_eq!(
        apps(&game_apps),
        [
            ("cad".into(), "matched files:*.FCStd".into()),
            ("orchestration".into(), "matched git".into()),
        ]
    );
    let cad = &game_apps["apps"][0];
    assert_eq!(
        (cad["label"].as_str(), cad["icon"].as_str()),
        (Some("Parts"), Some("cube"))
    );
    assert_eq!(cad["opens"], serde_json::json!(["*.FCStd"]));
    assert_eq!(cad["built"], false);
    // A broken block is reported, not fatal.
    let errors = game_apps["errors"].as_array().unwrap();
    assert!(
        errors.len() == 1 && errors[0].as_str().unwrap().contains("site://broken"),
        "{errors:?}"
    );

    // `notes`: calendar pinned first (a global app), then board by tag, video by its work.
    let (_, notes_apps) = get(addr, &token, "/api/projects/notes/apps").await;
    assert_eq!(
        apps(&notes_apps),
        [
            ("calendar".into(), "pinned".into()),
            ("board".into(), "matched tag:writing".into()),
        ]
    );
    // A renamed project isn't found by its folder's name.
    let (status, missing) = get(addr, &token, "/api/projects/game/apps").await;
    assert_eq!(status, 404, "{missing}");

    // Home: global and both-scope apps.
    let (_, home) = get(addr, &token, "/api/apps").await;
    assert_eq!(home["project"], Value::Null);
    // (with the stock chat and ticket board).
    assert_eq!(
        apps(&home),
        [
            ("board".into(), "home".into()),
            ("calendar".into(), "home".into()),
            ("chat".into(), "home".into()),
            ("orchestration".into(), "home".into()),
        ]
    );

    // The same over ws.
    let mut socket = connect(addr, &token).await;
    send(
        &mut socket,
        7,
        ClientCommand::AppsList {
            project: Some("notes".into()),
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(
            e,
            ServerMessage::Ok { id: 7, .. } | ServerMessage::Error { id: Some(7), .. }
        )
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Apps { list },
        ..
    }) = reply.last()
    else {
        panic!("{reply:?}")
    };
    assert_eq!(list.apps.len(), 2);
    assert_eq!(list.apps[0].site, "calendar");
}
