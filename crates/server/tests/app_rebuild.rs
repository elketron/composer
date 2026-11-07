//! Old app builds (ticket 0169): an app built with an sdk from before the `/h/` and `/p/` routes
//! says so there instead of showing nothing, and the listings flag it; `apps.rebuild` shows the
//! `ui-build` recipes it would run, and with `yes` runs them in the sandbox.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use reqwest::{Client, Method};
use serde_json::Value;
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

async fn call(addr: SocketAddr, method: Method, path: &str, token: &str) -> (u16, String) {
    let response = Client::new()
        .request(method, format!("http://{addr}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

async fn note(socket: &mut Socket, id: u64, yes: bool) -> String {
    send(socket, id, ClientCommand::AppsRebuild { all: false, yes }).await;
    let events = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    match events.last() {
        Some(ServerMessage::Ok {
            result: CommandResult::Note { text },
            ..
        }) => text.clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn an_old_build_says_so_and_rebuilds() {
    if !has("just") {
        return;
    }
    let (dir, server, _provider, addr) = start(vec![]).await;
    let sites = dir.path().join("sites");
    // Only this test's apps (the stock ones would need bun to build).
    for stock in ["chat", "orchestration"] {
        std::fs::remove_dir_all(sites.join(stock).join("ui")).unwrap();
    }
    // `old`: built before stamps; its ui-build makes a stamped build. `fresh`: current.
    write(&sites.join("old/index.md"), "---\ntitle: Old\n---\n# Old\n");
    write(&sites.join("old/ui/dist/index.html"), "<p>old build</p>");
    write(&sites.join("old/ui/dist/assets/a.js"), "let x = 1;");
    write(
        &sites.join("old/justfile"),
        "ui-build:\n    echo '<p>new build</p>' > ui/dist/index.html\n    echo 'd=\"composer-sdk:2\"' > ui/dist/assets/a.js\n",
    );
    write(
        &sites.join("fresh/index.md"),
        "---\ntitle: Fresh\n---\n# Fresh\n",
    );
    write(&sites.join("fresh/ui/dist/index.html"), "<p>fresh</p>");
    write(
        &sites.join("fresh/ui/dist/assets/a.js"),
        "d=\"composer-sdk:2\"",
    );
    write(&sites.join("fresh/justfile"), "ui-build:\n    echo fresh\n");
    let token = server.token.clone();

    // Opened on Home, the old build says it needs a rebuild.
    let (_, body) = call(addr, Method::POST, "/api/apps/old/open", &token).await;
    let url = serde_json::from_str::<Value>(&body).unwrap()["url"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, page) = call(addr, Method::GET, &url, "").await;
    assert_eq!(status, 409, "{page}");
    assert!(page.contains("older composer sdk"), "{page}");
    let (_, body) = call(addr, Method::GET, "/api/apps", &token).await;
    let home: Value = serde_json::from_str(&body).unwrap();
    let outdated = |site: &str| {
        home["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["site"] == site)
            .unwrap()["outdated"]
            .as_bool()
            .unwrap()
    };
    assert!(outdated("old") && !outdated("fresh"));

    // The plan: the old one only, nothing run.
    let mut socket = connect(addr, &token).await;
    let plan = note(&mut socket, 1, false).await;
    assert!(
        plan.contains("site://old") && !plan.contains("site://fresh"),
        "{plan}"
    );
    assert!(plan.contains("Nothing was run"), "{plan}");
    assert!(
        std::fs::read_to_string(sites.join("old/ui/dist/index.html"))
            .unwrap()
            .contains("old build")
    );

    // With yes: built, and served.
    let done = note(&mut socket, 2, true).await;
    assert_eq!(done.trim(), "old: built", "{done}");
    let (status, page) = call(addr, Method::GET, &url, "").await;
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("new build"), "{page}");
    assert!(
        note(&mut socket, 3, false)
            .await
            .contains("nothing to rebuild")
    );
}
