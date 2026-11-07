//! Opening a project (ticket 0165): clients `project.open` it (by name or folder); the first open
//! runs its apps' approved `on_open` recipes once (an unapproved one is reported, not run) and
//! starts a watch whose changes reach the clients that opened it (not `node_modules`); a dropped
//! connection closes what it opened, the last close stops it. Home lists `tile` apps with their
//! `tile.md`.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::path::Path;
use std::time::Duration;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use composer_server::actions::{Caller, Justfile};
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

/// Events that arrived while waiting for replies, so none is lost.
static SEEN: std::sync::Mutex<Vec<ServerMessage>> = std::sync::Mutex::new(Vec::new());

/// The reply to `command`, sent as `id` (the events before it are kept in [`SEEN`]).
async fn ask(
    socket: &mut Socket,
    id: u64,
    command: ClientCommand,
) -> Result<CommandResult, String> {
    send(socket, id, command).await;
    let mut events = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    let reply = events.pop();
    SEEN.lock().unwrap().extend(events);
    match reply.as_ref() {
        Some(ServerMessage::Ok { result, .. }) => Ok(result.clone()),
        Some(ServerMessage::Error { message, .. }) => Err(message.clone()),
        other => panic!("{other:?}"),
    }
}

async fn clients(socket: &mut Socket, id: u64) -> Vec<(String, u32)> {
    let Ok(CommandResult::OpenProjects { projects }) =
        ask(socket, id, ClientCommand::ProjectsOpen).await
    else {
        panic!()
    };
    projects.into_iter().map(|p| (p.name, p.clients)).collect()
}

#[tokio::test]
async fn a_project_opens_once_and_closes_with_its_last_client() {
    if !has("just") {
        return;
    }
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    let game = root.join("game");
    write(&game.join("arm.FCStd"), "");
    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let cad = dir.path().join("sites/cad");
    write(
        &cad.join("index.md"),
        "---\ntitle: CAD\napp:\n  scope: project\n  when: [\"files:*.FCStd\"]\n  on_open: [refresh, later]\n---\n# CAD\n",
    );
    write(
        &cad.join("justfile"),
        "refresh:\n    echo refreshed >> \"$APP_DATA/refreshed.txt\"\n\nlater:\n    echo not yet\n",
    );
    let justfile = Justfile::load(&cad).await.unwrap();
    server
        .approvals
        .approve("cad", "refresh", &justfile.hash("refresh"), Caller::App)
        .unwrap();
    // Home: a tile app with its tile.
    write(
        &dir.path().join("sites/board/index.md"),
        "---\ntitle: Board\napp:\n  scope: both\n  tile: true\n---\n# Board\n",
    );
    write(
        &dir.path().join("sites/board/tile.md"),
        "3 open, 1 waiting\n",
    );

    let mut first = connect(addr, &server.token).await;
    let mut second = connect(addr, &server.token).await;
    let opened = ask(
        &mut first,
        1,
        ClientCommand::ProjectOpen {
            project: "game".into(),
        },
    )
    .await;
    assert_eq!(
        opened,
        Ok(CommandResult::ProjectOpened {
            project: "game".into(),
            first: true
        })
    );
    // By its folder, from another client: the same project, not the first open.
    let again = ask(
        &mut second,
        2,
        ClientCommand::ProjectOpen {
            project: game.display().to_string(),
        },
    )
    .await;
    assert_eq!(
        again,
        Ok(CommandResult::ProjectOpened {
            project: "game".into(),
            first: false
        })
    );
    // Twice from the same client counts once.
    ask(
        &mut first,
        3,
        ClientCommand::ProjectOpen {
            project: "game".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(clients(&mut first, 4).await, [("game".into(), 2)]);

    // `on_open`: refresh ran (once), later needs approval.
    let mut ran = Vec::new();
    while ran.len() < 2 {
        let seen = SEEN.lock().unwrap().pop();
        let event = match seen {
            Some(seen) => seen,
            None => tokio::time::timeout(Duration::from_secs(20), next_event(&mut first))
                .await
                .expect("on_open events"),
        };
        if let ServerMessage::OnOpenRan {
            project,
            app,
            recipe,
            status,
            ..
        } = event
        {
            ran.push((project, app, recipe, status));
        }
    }
    assert_eq!(
        ran,
        [
            ("game".into(), "cad".into(), "refresh".into(), "ok".into()),
            (
                "game".into(),
                "cad".into(),
                "later".into(),
                "needs approval".into()
            ),
        ]
    );
    let refreshed = dir.path().join("state/projects/game/cad/refreshed.txt");
    assert_eq!(std::fs::read_to_string(&refreshed).unwrap(), "refreshed\n");

    // Changes: the project's own files, not its dependencies.
    write(&game.join("node_modules/x/index.js"), "");
    write(&game.join("parts/new.FCStd"), "");
    let changed = loop {
        let event = tokio::time::timeout(Duration::from_secs(10), next_event(&mut first))
            .await
            .expect("a project.changed");
        if let ServerMessage::ProjectChanged { project, paths } = event {
            assert_eq!(project, "game");
            if paths.iter().any(|p| p.ends_with("new.FCStd")) {
                break paths;
            }
        }
    };
    assert!(
        changed.iter().all(|p| !p.contains("node_modules")),
        "{changed:?}"
    );

    // The second client goes: one left; the first closes: none.
    drop(second);
    let mut left = Vec::new();
    for _ in 0..50 {
        left = clients(&mut first, 5).await;
        if left == [("game".to_string(), 1)] {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(left, [("game".into(), 1)]);
    ask(
        &mut first,
        6,
        ClientCommand::ProjectClose {
            project: "game".into(),
        },
    )
    .await
    .unwrap();
    assert!(clients(&mut first, 7).await.is_empty());
    assert_eq!(
        std::fs::read_to_string(&refreshed).unwrap(),
        "refreshed\n",
        "on_open ran once"
    );

    // An unknown project says so.
    let unknown = ask(
        &mut first,
        8,
        ClientCommand::ProjectOpen {
            project: "nope".into(),
        },
    )
    .await;
    assert!(unknown.unwrap_err().contains("no project `nope`"));

    // Home: the tile.
    let home: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/apps"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let board = home["apps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["site"] == "board")
        .unwrap();
    assert_eq!(board["tile_text"], "3 open, 1 waiting");
}
