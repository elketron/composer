//! A mount (a symlink inside a site into the projects dir from config) is part of the web:
//! files landing on the project side reach subscribers as `page.changed` with the site's URL,
//! both for mounts that exist at startup and for ones created while running, and their pages
//! become searchable. A symlink into a folder outside the allowed scope is refused: changes
//! there light nothing up and reading through it errors.

mod support;

use std::sync::Arc;
use std::time::Duration;

use composer_core::{Query, SiteUrl};
use composer_protocol::{ClientCommand, ServerMessage};
use composer_server::Config;
use support::*;

fn changed_events(events: &[ServerMessage]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::PageChanged { url, kind, .. } => {
                Some((url.clone(), format!("{kind:?}")))
            }
            _ => None,
        })
        .collect()
}

fn search(server: &composer_server::Server, query: &str) -> Vec<String> {
    server
        .index
        .read()
        .expect("index lock")
        .search(&Query::parse(query))
        .into_iter()
        .map(|h| h.url.to_string())
        .collect()
}

/// A running server whose `projects.dir` is a fresh temp folder, with a subscribed socket.
async fn listen() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Arc<composer_server::Server>,
    Socket,
) {
    let projects = tempfile::tempdir().unwrap();
    let config = Config::parse(&format!(
        "[projects]\ndir = \"{}\"\n",
        projects.path().display()
    ))
    .unwrap();
    let (home, server, _p, addr) = start_with(config, vec![]).await;
    // The watcher comes up when serve runs; let it, as the other watcher tests do.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::Subscribe {
            urls: vec!["site://plans/**".into()],
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    (home, projects, server, socket)
}

#[tokio::test]
async fn a_mount_created_while_running_carries_events_and_search() {
    let (home, projects, server, mut socket) = listen().await;
    // The mount appears after the watcher came up: the dynamic attach must catch it.
    let mounted = projects.path().join("ref2img");
    std::fs::create_dir_all(&mounted).unwrap();
    std::os::unix::fs::symlink(&mounted, home.path().join("sites/plans/toolbox")).unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;

    std::fs::write(
        mounted.join("landing.md"),
        "---\ntitle: Landing\ntype: mount-landing\n---\n# Landing\n",
    )
    .unwrap();
    until(&mut socket, |e| {
        matches!(e, ServerMessage::PageChanged { url, .. } if url == "site://plans/toolbox/landing.md")
    })
    .await;

    // Pages under an allowed mount are searchable (decision: indexing follows allowed mounts).
    let hits = search(&server, "type:mount-landing");
    assert_eq!(hits, ["site://plans/toolbox/landing.md"]);

    // And readable through the web's own address for it.
    let page = server
        .workspace
        .read_page(&SiteUrl::parse("site://plans/toolbox/landing").unwrap())
        .unwrap();
    assert_eq!(page.url.to_string(), "site://plans/toolbox/landing.md");
}

#[tokio::test]
async fn a_mount_existing_at_server_start_is_scanned() {
    // The mount is there when the watcher comes up: the startup scan must attach to it
    // (the test builds its home before starting the server, unlike `listen`).
    let projects = tempfile::tempdir().unwrap();
    let home_dir = tempfile::tempdir().unwrap();
    let home = composer_server::Home::new(home_dir.path());
    composer_server::init_home(&home).await.unwrap();
    let mounted = projects.path().join("ref2img");
    std::fs::create_dir_all(&mounted).unwrap();
    std::os::unix::fs::symlink(&mounted, home.sites().join("plans/mounted")).unwrap();

    let config = Config::parse(&format!(
        "[projects]\ndir = \"{}\"\n",
        projects.path().display()
    ))
    .unwrap();
    let provider = Arc::new(MockProvider::new(vec![]));
    let server = composer_server::Server::new(home, config, provider).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = server.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::Subscribe {
            urls: vec!["site://plans/**".into()],
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;

    // A file landing on the project side (as a stage would drop its render) lights up
    // under the site's address.
    std::fs::write(
        mounted.join("new.md"),
        "---\ntitle: New\ntype: mount-scan-new\n---\n# New\n",
    )
    .unwrap();
    until(&mut socket, |e| {
        matches!(e, ServerMessage::PageChanged { url, .. } if url == "site://plans/mounted/new.md")
    })
    .await;
}

#[tokio::test]
async fn a_mount_outside_the_allowed_scope_is_refused() {
    let (home, _projects, server, mut socket) = listen().await;
    let outside = tempfile::tempdir().unwrap();

    std::os::unix::fs::symlink(outside.path(), home.path().join("sites/plans/elsewhere")).unwrap();
    std::fs::write(outside.path().join("stray.md"), "# Stray\n").unwrap();

    // Nothing lights up for the refused mount: a ping answers with no `page.changed` before it.
    tokio::time::sleep(Duration::from_millis(400)).await;
    send(&mut socket, 2, ClientCommand::SessionList).await;
    let rest = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    assert_eq!(changed_events(&rest), [], "{rest:?}");

    // Reading through a refused mount errors, with an error that names what to do.
    let err = server
        .workspace
        .read_page(&SiteUrl::parse("site://plans/elsewhere/stray").unwrap())
        .unwrap_err();
    assert!(
        err.to_string().contains("symlink that points outside"),
        "{err}"
    );
    assert!(
        !search(&server, "Stray").contains(&"site://plans/elsewhere/stray.md".to_string()),
        "a refused mount contributes nothing to the index"
    );
}
// --- Review 0141: the HTTP file routes guard the mount gate too (R1, M1) ---

#[tokio::test]
async fn http_routes_refuse_a_mount_outside_the_allowed_scope() {
    let projects = tempfile::tempdir().unwrap();
    let config = Config::parse(&format!(
        "[projects]\ndir = \"{}\"\n",
        projects.path().display()
    ))
    .unwrap();
    let (home, server, _p, addr) = start_with(config, vec![]).await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "top secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), home.path().join("sites/plans/elsewhere")).unwrap();

    let client = reqwest::Client::new();
    let base = format!("http://{addr}/api/files/plans/elsewhere");

    let get = client
        .get(format!("{base}/secret.txt"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        get.status(),
        reqwest::StatusCode::FORBIDDEN,
        "GET through a refused mount"
    );
    let body: serde_json::Value = get.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("symlink that points outside"),
        "{body}"
    );

    let put = client
        .put(format!("{base}/planted.txt"))
        .bearer_auth(&server.token)
        .body("planted")
        .send()
        .await
        .unwrap();
    assert_eq!(
        put.status(),
        reqwest::StatusCode::FORBIDDEN,
        "PUT through a refused mount"
    );
    assert!(
        !outside.path().join("planted.txt").exists(),
        "a refused PUT wrote through the mount"
    );

    // serve_app (`/<site>/<path>`) answers 404 for an unbuilt site, and serves nothing
    // through a refused mount either: a built app whose dist folder is a real folder is
    // unaffected, a symlink inside dist that escapes is refused.
    let dist = home.path().join("sites/plans/ui/dist");
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::write(dist.join("index.html"), "<html>app</html>").unwrap();
    std::fs::create_dir_all(outside.path().join("deep")).unwrap();
    std::fs::write(outside.path().join("deep/blob.txt"), "blobbed").unwrap();
    // plans/ui/dist/link -> the refused mount's tree (plans/elsewhere is the refused link,
    // so a link to it from dist reaches the same refused path through the gate).
    std::os::unix::fs::symlink(
        home.path().join("sites/plans/elsewhere/deep"),
        dist.join("link"),
    )
    .unwrap();
    // The dist link itself points into plans/elsewhere, which is refused: serve_app must refuse too.
    let resp = client
        .get(format!(
            "http://{addr}/plans/link/blob.txt?token={}",
            server.token
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "serve_app served through a refused mount"
    );
}

#[tokio::test]
async fn http_routes_read_and_write_through_an_allowed_mount() {
    let projects = tempfile::tempdir().unwrap();
    let config = Config::parse(&format!(
        "[projects]\ndir = \"{}\"\n",
        projects.path().display()
    ))
    .unwrap();
    let (home, server, _p, addr) = start_with(config, vec![]).await;
    let mounted = projects.path().join("ref2img");
    std::fs::create_dir_all(&mounted).unwrap();
    std::fs::write(mounted.join("view.json"), "{\"n\": 1}").unwrap();
    std::os::unix::fs::symlink(&mounted, home.path().join("sites/plans/toolbox")).unwrap();

    let client = reqwest::Client::new();
    let base = format!("http://{addr}/api/files/plans/toolbox");
    let get = client
        .get(format!("{base}/view.json"))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        get.status(),
        reqwest::StatusCode::OK,
        "GET through an allowed mount"
    );
    assert_eq!(get.text().await.unwrap(), "{\"n\": 1}");

    let put = client
        .put(format!("{base}/out.json"))
        .bearer_auth(&server.token)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        put.status(),
        reqwest::StatusCode::OK,
        "PUT through an allowed mount"
    );
    assert!(
        mounted.join("out.json").exists(),
        "the PUT did not land on the project side"
    );
}
