//! A mount (a symlink inside a site into a folder a site names with `folder:`) is part of the
//! web: files landing on the folder's side reach subscribers as `page.changed` with the site's URL,
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

/// Names `folder` as the `plans` site's folder.
fn plans_folder(sites: &std::path::Path, folder: &std::path::Path) {
    let index = sites.join("plans/index.md");
    let text = std::fs::read_to_string(&index).unwrap();
    let text = text.replacen("---\n", &format!("---\nfolder: {}\n", folder.display()), 1);
    std::fs::write(&index, text).unwrap();
}

/// A running server whose `plans` site names a fresh temp folder, with a subscribed socket.
async fn listen() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Arc<composer_server::Server>,
    Socket,
) {
    let projects = tempfile::tempdir().unwrap();
    let (home, server, _p, addr) = start_with(Config::defaults(), vec![]).await;
    plans_folder(&home.path().join("sites"), projects.path());
    approve_folder(&server, "plans");
    server.reindex();
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
        .web
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

    plans_folder(&home.sites(), projects.path());
    // Approved before the server starts: the mount counts from the start.
    composer_server::actions::Approvals::load(&home.approvals_file())
        .unwrap()
        .approve(
            "plans",
            composer_server::agent::FOLDER_APPROVAL,
            &composer_server::agent::folder_hash(&projects.path().canonicalize().unwrap()),
        )
        .unwrap();
    let provider = Arc::new(MockProvider::new(vec![]));
    let server = composer_server::Server::new(home, Config::defaults(), provider).unwrap();
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
        .web
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
