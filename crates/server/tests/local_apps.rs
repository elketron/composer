//! Project-local apps (ticket 0166): `<project>/.composer/apps/<app>/` is listed with the
//! project's apps but off until allowed (`app.allow`); allowed, it is served and runs its own
//! recipes on the project (its allowance is their approval), writing its project and data but no
//! site; a changed justfile turns it off again; `app.promote` copies it into the workspace, whose
//! copy wins from then on.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
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

async fn command(socket: &mut Socket, id: u64, command: ClientCommand) -> Result<String, String> {
    send(socket, id, command).await;
    let events = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    match events.last() {
        Some(ServerMessage::Ok {
            result: CommandResult::Note { text },
            ..
        }) => Ok(text.clone()),
        Some(ServerMessage::Error { message, .. }) => Err(message.clone()),
        other => panic!("{other:?}"),
    }
}

/// `viewer`'s `(why, local)` in `game`'s apps.
async fn viewer(addr: SocketAddr, token: &str) -> Option<(String, bool)> {
    let (_, body) = call(addr, Method::GET, "/api/projects/game/apps", token, None).await;
    let list: Value = serde_json::from_str(&body).unwrap();
    list["apps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["site"] == "viewer")
        .map(|a| {
            (
                a["why"].as_str().unwrap().to_string(),
                a["local"].as_bool().unwrap(),
            )
        })
}

#[tokio::test]
async fn a_project_local_app_runs_once_allowed() {
    if !has("just") {
        return;
    }
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    let game = root.join("game");
    let app = game.join(".composer/apps/viewer");
    write(
        &app.join("index.md"),
        "---\ntitle: Viewer\napp:\n  scope: project\n---\n# Viewer\n",
    );
    write(
        &app.join("justfile"),
        "probe:\n    echo seen > \"$APP_DATA/probe.txt\"\n",
    );
    write(&app.join("ui/dist/index.html"), "<p>the viewer</p>");
    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let token = server.token.clone();
    let mut socket = connect(addr, &token).await;
    let open = || async {
        call(
            addr,
            Method::POST,
            "/api/projects/game/apps/viewer/open",
            &token,
            None,
        )
        .await
    };

    // Found, but off.
    assert_eq!(
        viewer(addr, &token).await,
        Some(("local, not allowed".into(), true))
    );
    let (status, body) = open().await;
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("composer app allow game viewer"), "{body}");

    // Allowed: served, and it runs its recipe without another approval.
    let said = command(
        &mut socket,
        1,
        ClientCommand::AppAllow {
            project: "game".into(),
            app: "viewer".into(),
        },
    )
    .await
    .unwrap();
    assert!(said.contains("allowed `viewer`"), "{said}");
    assert_eq!(viewer(addr, &token).await, Some(("local".into(), true)));
    let (status, body) = open().await;
    assert_eq!(status, 200, "{body}");
    let me = serde_json::from_str::<Value>(&body).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, page) = call(addr, Method::GET, "/p/game/viewer/", &me, None).await;
    assert_eq!((status, page.contains("the viewer")), (200, true), "{page}");
    let (status, body) = call(
        addr,
        Method::POST,
        "/p/game/viewer/api/actions/viewer/probe",
        &me,
        Some(json!({ "args": [] })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["status"],
        "ok"
    );
    assert!(
        dir.path()
            .join("state/projects/game/viewer/probe.txt")
            .is_file()
    );
    // It has no site to write.
    let (status, body) = call(
        addr,
        Method::PUT,
        "/p/game/viewer/api/pages/viewer/x.md",
        &me,
        Some(json!({ "content": "x" })),
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("no site in the web"), "{body}");

    // A changed justfile: off again.
    write(&app.join("justfile"), "probe:\n    echo changed\n");
    assert_eq!(
        viewer(addr, &token).await,
        Some(("local, not allowed".into(), true))
    );
    assert_eq!(open().await.0, 404);

    // Promoted: the workspace's copy wins (and has no `when`: the note says to pin it).
    let said = command(
        &mut socket,
        2,
        ClientCommand::AppPromote {
            project: "game".into(),
            app: "viewer".into(),
        },
    )
    .await
    .unwrap();
    assert!(said.contains("pin it to `game`"), "{said}");
    assert!(dir.path().join("sites/viewer/index.md").is_file());
    assert_eq!(
        viewer(addr, &token).await,
        None,
        "shadowed, and not attached"
    );
    let (_, body) = call(addr, Method::GET, "/api/projects/game/apps", &token, None).await;
    assert!(body.contains("shadowed by site://viewer"), "{body}");
    let again = command(
        &mut socket,
        3,
        ClientCommand::AppPromote {
            project: "game".into(),
            app: "viewer".into(),
        },
    )
    .await;
    assert!(again.unwrap_err().contains("already has site://viewer"));
}
