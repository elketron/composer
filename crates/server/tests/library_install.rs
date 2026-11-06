//! Installing sites from the library and updating them (ticket 0152): install copies a library
//! site's definition into one workspace and records the commit; update merges per file against
//! that commit; stock sites take the same path.

use std::path::Path;

use composer_server::UpdateOutcome;
use composer_server::library::{Library, SiteName, WorkspaceLibrary};
use composer_server::{Home, WorkspaceHome, WorkspaceName, init_home, init_workspace, update_home};

/// A home with workspaces `default` and `b`, and a library site `cad` (index, justfile, a stage).
async fn home() -> (tempfile::TempDir, Home, WorkspaceHome, WorkspaceHome) {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let b = home.workspace(&WorkspaceName::new("b").unwrap());
    init_workspace(&b).await.unwrap();
    let cad = home.library().join("cad");
    std::fs::create_dir_all(cad.join("stages")).unwrap();
    std::fs::write(cad.join("index.md"), lines("cad", 12)).unwrap();
    std::fs::write(cad.join("justfile"), "build:\n    @echo built\n").unwrap();
    std::fs::write(cad.join("stages/01_plan.md"), "# Plan\n").unwrap();
    Library::of(&home).record("publish: cad").unwrap();
    let a = home.workspace(&WorkspaceName::default_workspace());
    (dir, home, a, b)
}

/// `n` numbered lines, so edits far apart merge cleanly.
fn lines(name: &str, n: usize) -> String {
    (1..=n).map(|i| format!("{name} line {i}\n")).collect()
}

fn site(name: &str) -> SiteName {
    SiteName::new(name).unwrap()
}

fn read(workspace: &WorkspaceHome, rel: &str) -> String {
    std::fs::read_to_string(workspace.sites().join(rel)).unwrap()
}

fn library_edit(home: &Home, rel: &str, edit: impl FnOnce(String) -> Option<String>) {
    let path = home.library().join(rel);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    match edit(text) {
        Some(new) => {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, new).unwrap();
        }
        None => std::fs::remove_file(&path).unwrap(),
    }
    Library::of(home).record("publish: cad again").unwrap();
}

#[tokio::test]
async fn each_workspace_installs_its_own_copy() {
    let (_dir, home, a, b) = home().await;
    let from_a = WorkspaceLibrary::of(&a).unwrap();
    let installed = from_a
        .install(&site("cad"), None, &from_a.approvals().unwrap())
        .await
        .unwrap();
    assert_eq!(installed.files, 3);
    assert!(!installed.has_ui);
    WorkspaceLibrary::of(&b)
        .unwrap()
        .install(
            &site("cad"),
            Some(&site("my-cad")),
            &WorkspaceLibrary::of(&b).unwrap().approvals().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read(&a, "cad/stages/01_plan.md"), "# Plan\n");
    assert!(b.sites().join("my-cad/justfile").is_file());
    assert!(!b.sites().join("cad").exists());

    // A change in one copy stays in that workspace.
    std::fs::write(a.sites().join("cad/stages/01_plan.md"), "# Plan of a\n").unwrap();
    assert_eq!(read(&b, "my-cad/stages/01_plan.md"), "# Plan\n");

    // The library lists who installed what.
    let list = from_a.list().unwrap();
    let cad = list.iter().find(|s| s.site.as_str() == "cad").unwrap();
    assert_eq!(cad.installed, [(site("cad"), false)]);

    // Refusals: a name the workspace has, a site the library hasn't.
    let error = from_a
        .install(&site("cad"), None, &from_a.approvals().unwrap())
        .await
        .unwrap_err();
    assert!(error.contains("already has a site `cad`"), "{error}");
    let error = from_a
        .install(&site("nope"), None, &from_a.approvals().unwrap())
        .await
        .unwrap_err();
    assert!(error.contains("no site `nope`"), "{error}");
    // A standalone workspace has no library.
    let alone = tempfile::tempdir().unwrap();
    assert!(WorkspaceLibrary::of(&WorkspaceHome::new(alone.path())).is_err());
    let _ = home;
}

#[tokio::test]
async fn update_replaces_merges_and_keeps_conflicts_apart() {
    let (_dir, home, a, _b) = home().await;
    let from_a = WorkspaceLibrary::of(&a).unwrap();
    from_a
        .install(&site("cad"), None, &from_a.approvals().unwrap())
        .await
        .unwrap();
    // Here: line 1 of the index changed, the stage untouched, the justfile changed.
    let index = read(&a, "cad/index.md").replace("cad line 1\n", "cad line 1, a's\n");
    std::fs::write(a.sites().join("cad/index.md"), index).unwrap();
    std::fs::write(
        a.sites().join("cad/justfile"),
        "build:\n    @echo a's build\n",
    )
    .unwrap();
    // Library: line 12 of the index, the stage, the justfile (same line: conflict), a new file,
    // and a removed one.
    library_edit(&home, "cad/index.md", |t| {
        Some(t.replace("cad line 12\n", "cad line 12, new\n"))
    });
    library_edit(&home, "cad/stages/01_plan.md", |_| {
        Some("# Plan v2\n".into())
    });
    library_edit(&home, "cad/justfile", |_| {
        Some("build:\n    @echo v2\n".into())
    });
    library_edit(&home, "cad/stages/02_build.md", |_| {
        Some("# Build\n".into())
    });
    let before = std::fs::read(a.sites().join("cad/justfile")).unwrap();

    let dry = from_a
        .update(true, &from_a.approvals().unwrap())
        .await
        .unwrap();
    assert_eq!(dry.len(), 4);
    assert!(
        !a.sites().join("cad/stages/02_build.md").exists(),
        "a dry run writes nothing"
    );
    assert_eq!(from_a.behind(), 1);

    let items = from_a
        .update(false, &from_a.approvals().unwrap())
        .await
        .unwrap();
    let outcome = |file: &str| {
        items
            .iter()
            .find(|i| i.path.ends_with(file))
            .map(|i| i.outcome.clone())
    };
    assert_eq!(outcome("cad/index.md"), Some(UpdateOutcome::Merged));
    assert_eq!(
        outcome("cad/stages/01_plan.md"),
        Some(UpdateOutcome::Replaced)
    );
    assert_eq!(
        outcome("cad/stages/02_build.md"),
        Some(UpdateOutcome::Added)
    );
    assert!(matches!(
        outcome("cad/justfile"),
        Some(UpdateOutcome::Conflict { .. })
    ));
    let index = read(&a, "cad/index.md");
    assert!(index.contains("cad line 1, a's") && index.contains("cad line 12, new"));
    assert_eq!(
        std::fs::read(a.sites().join("cad/justfile")).unwrap(),
        before
    );
    let review = a.updates().join("sites/cad/justfile");
    assert!(std::fs::read_to_string(review).unwrap().contains("<<<<<<<"));
    assert_eq!(from_a.behind(), 0);
    assert!(
        from_a
            .update(false, &from_a.approvals().unwrap())
            .await
            .unwrap()
            .is_empty(),
        "up to date"
    );

    // Removed from the library and unchanged here: removed.
    library_edit(&home, "cad/stages/02_build.md", |_| None);
    let items = from_a
        .update(false, &from_a.approvals().unwrap())
        .await
        .unwrap();
    assert_eq!(items[0].outcome, UpdateOutcome::Removed);
    assert!(!a.sites().join("cad/stages/02_build.md").exists());
}

#[tokio::test]
async fn sites_not_from_the_library_are_never_touched() {
    let (_dir, home, a, _b) = home().await;
    // The workspace's own `designer` and a `cad` of its own (not installed).
    std::fs::create_dir_all(a.sites().join("cad")).unwrap();
    std::fs::write(a.sites().join("cad/index.md"), "# my own cad\n").unwrap();
    library_edit(&home, "cad/index.md", |_| Some("# library cad v2\n".into()));
    update_home(&home, false).await.unwrap();
    assert_eq!(read(&a, "cad/index.md"), "# my own cad\n");
}

#[tokio::test]
async fn stock_sites_update_from_the_library() {
    let (_dir, home, a, b) = home().await;
    // New workspaces installed their stock sites from the library.
    let installed = WorkspaceLibrary::of(&b).unwrap().installed().unwrap();
    assert!(installed.get(&site("orchestration")).is_some());
    // A library edit to a stock site reaches both workspaces on update.
    library_edit(&home, "home/index.md", |t| {
        Some(format!("{t}\nFrom the library.\n"))
    });
    update_home(&home, false).await.unwrap();
    for workspace in [&a, &b] {
        assert!(read(workspace, "home/index.md").contains("From the library."));
    }
}

#[tokio::test]
async fn a_workspace_from_before_the_library_joins_on_update() {
    let (_dir, home, a, _b) = home().await;
    std::fs::remove_file(a.library_file()).unwrap();
    update_home(&home, false).await.unwrap();
    let installed = WorkspaceLibrary::of(&a).unwrap().installed().unwrap();
    for stock in ["agents", "home", "orchestration"] {
        let record = installed.get(&site(stock)).unwrap();
        assert_eq!(record.from, site(stock));
    }
    // Only stock sites join: a workspace's own sites don't.
    assert!(installed.get(&site("cad")).is_none());
    let _ = Path::new("");
}

mod support;

#[tokio::test]
async fn clients_list_install_and_update_through_their_workspace() {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use composer_protocol::{ClientCommand, ClientRequest, CommandResult, ServerMessage};
    use composer_server::{Host, Server};
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (_dir, home, a, b) = home().await;
    let mut servers = BTreeMap::new();
    for name in home.workspace_names().unwrap() {
        let workspace = home.workspace(&name);
        let config = workspace.load_config().unwrap();
        let provider = Arc::new(support::MockProvider::new(vec![]));
        servers.insert(name, Server::new(workspace, config, provider).unwrap());
    }
    let host = Arc::new(Host::new(home.clone(), servers).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = host.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    let b_server = host
        .workspace(&WorkspaceName::new("b").unwrap())
        .unwrap()
        .clone();
    while b_server.parent_url().is_none() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let url = format!("ws://{addr}/w/b/ws?token={}", b_server.token);
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let mut id = 0;
    let mut send = async |command: ClientCommand| -> Result<CommandResult, String> {
        id += 1;
        let json = serde_json::to_string(&ClientRequest { id, command }).unwrap();
        socket.send(Message::Text(json.into())).await.unwrap();
        loop {
            let Some(Ok(Message::Text(text))) = socket.next().await else {
                panic!("socket closed");
            };
            match serde_json::from_str::<ServerMessage>(&text).unwrap() {
                ServerMessage::Ok { id: got, result } if got == id => return Ok(result),
                ServerMessage::Error {
                    id: Some(got),
                    message,
                } if got == id => return Err(message),
                _ => {}
            }
        }
    };

    let listed = |result: Result<CommandResult, String>| -> Vec<String> {
        let Ok(CommandResult::Library { sites }) = result else {
            panic!("{result:?}")
        };
        let cad = sites.into_iter().find(|s| s.site == "cad").unwrap();
        cad.installed
            .into_iter()
            .map(|i| format!("{}:{}", i.site, i.behind))
            .collect()
    };
    assert!(listed(send(ClientCommand::LibraryList).await).is_empty());
    let installed = send(ClientCommand::LibraryInstall {
        site: "cad".into(),
        name: None,
    })
    .await;
    assert!(
        matches!(
            installed,
            Ok(CommandResult::LibraryInstalled { files: 3, .. })
        ),
        "{installed:?}"
    );
    assert_eq!(
        listed(send(ClientCommand::LibraryList).await),
        ["cad:false"]
    );
    // Only b got it.
    assert!(b.sites().join("cad/index.md").is_file());
    assert!(!a.sites().join("cad").exists());
    // A bad name is refused before anything is written.
    let refused = send(ClientCommand::LibraryInstall {
        site: "../a/sites/home".into(),
        name: None,
    })
    .await;
    assert!(refused.is_err());

    library_edit(&home, "cad/stages/01_plan.md", |_| {
        Some("# Plan v2\n".into())
    });
    assert_eq!(listed(send(ClientCommand::LibraryList).await), ["cad:true"]);
    let Ok(CommandResult::LibraryUpdated { changes }) =
        send(ClientCommand::LibraryUpdate { dry_run: false }).await
    else {
        panic!()
    };
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].outcome, "replaced");
    assert_eq!(read(&b, "cad/stages/01_plan.md"), "# Plan v2\n");
}
