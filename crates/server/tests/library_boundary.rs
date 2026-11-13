//! The library review suite (ticket 0155): with a library and two workspaces, install, update,
//! publish and templates each write only where they may. Publishing is the only write that leaves
//! a workspace; nothing reads another workspace's content; a published site never carries pages;
//! nothing outside a site gets into one.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use composer_server::library::{Library, SiteName, WorkspaceLibrary};
use composer_server::{
    Home, TemplateName, WorkspaceHome, WorkspaceName, create_workspace, init_home, init_workspace,
    update_home,
};

struct Fixture {
    _dir: tempfile::TempDir,
    home: Home,
    a: WorkspaceHome,
    b: WorkspaceHome,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let b = home.workspace(&WorkspaceName::new("b").unwrap());
    init_workspace(&b).await.unwrap();
    let a = home.workspace(&WorkspaceName::default_workspace());
    // A site of a's with a definition and content, and one of b's that holds a secret.
    let write = |root: &Path, rel: &str, text: &str| {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        &a.sites(),
        "cad/index.md",
        "---\ntitle: CAD\nlibrary: [tools/]\n---\n# CAD\n",
    );
    write(&a.sites(), "cad/stages/01_plan.md", "# Plan\n");
    write(&a.sites(), "cad/tools/slice.py", "print()\n");
    write(
        &a.sites(),
        "cad/printer/index.md",
        "---\ntype: work\n---\n# a's work\n",
    );
    write(&b.sites(), "secret/index.md", "# b's secret\n");
    let templates = home.global().join("workspace-templates");
    write(&templates, "3d.toml", "sites = [\"cad\"]\n");
    Fixture {
        _dir: dir,
        home,
        a,
        b,
    }
}

/// Every file under `root` with its contents (symlinks as their target).
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                files.insert(rel, target.display().to_string().into_bytes());
            } else if meta.is_dir() {
                stack.push(path);
            } else {
                files.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

fn site(name: &str) -> SiteName {
    SiteName::new(name).unwrap()
}

fn publish(workspace: &WorkspaceHome, name: &str) -> Result<(), String> {
    let library = WorkspaceLibrary::of(workspace).unwrap();
    let plan = library.plan_publish(&site(name), None)?;
    library.publish(&site(name), None, &plan.digest).map(|_| ())
}

#[tokio::test]
async fn install_update_and_templates_never_write_outside_their_workspace() {
    let f = fixture().await;
    publish(&f.a, "cad").unwrap();
    let library_head = Library::of(&f.home).head().unwrap();
    let a_before = snapshot(f.a.root());
    let library_before = snapshot(f.home.library().as_path());

    // b installs, updates, and a new workspace comes from a template.
    let b = WorkspaceLibrary::of(&f.b).unwrap();
    b.install(&site("cad"), None, &b.approvals().unwrap())
        .await
        .unwrap();
    b.update(false, &b.approvals().unwrap()).await.unwrap();
    let c = create_workspace(
        &f.home,
        &WorkspaceName::new("c").unwrap(),
        Some(&TemplateName::new("3d").unwrap()),
    )
    .await
    .unwrap();
    assert!(c.sites().join("cad/tools/slice.py").is_file());

    assert_eq!(snapshot(f.a.root()), a_before, "a untouched");
    assert_eq!(
        snapshot(f.home.library().as_path()),
        library_before,
        "the library untouched"
    );
    assert_eq!(Library::of(&f.home).head().unwrap(), library_head);
    // The work stayed in a.
    assert!(!f.b.sites().join("cad/printer").exists());
    assert!(!c.sites().join("cad/printer").exists());
}

#[tokio::test]
async fn publishing_writes_the_library_and_nothing_of_the_other_workspace() {
    let f = fixture().await;
    let b_before = snapshot(f.b.root());
    publish(&f.a, "cad").unwrap();
    assert_eq!(snapshot(f.b.root()), b_before);
    let published = snapshot(&f.home.library().join("cad"));
    let names: Vec<String> = published.keys().map(|p| p.display().to_string()).collect();
    assert_eq!(names, ["index.md", "tools/slice.py"]);
}

#[tokio::test]
async fn nothing_from_outside_a_site_gets_into_its_definition() {
    let f = fixture().await;
    // A link inside a's site to b's secret site, and an extra that climbs out.
    std::os::unix::fs::symlink(f.b.sites().join("secret"), f.a.sites().join("cad/tools/b"))
        .unwrap();
    publish(&f.a, "cad").unwrap();
    assert!(!f.home.library().join("cad/tools/b").exists());
    let library = snapshot(f.home.library().as_path());
    assert!(
        !library.values().any(|v| v == b"# b's secret\n"),
        "b's content reached the library"
    );

    std::fs::write(
        f.a.sites().join("cad/index.md"),
        "---\nlibrary: [\"../../../b/sites/secret\"]\n---\n# CAD\n",
    )
    .unwrap();
    let error = publish(&f.a, "cad").unwrap_err();
    assert!(error.contains("not a path inside the site"), "{error}");
}

#[tokio::test]
async fn a_stock_update_reaches_each_workspace_without_crossing_between_them() {
    let f = fixture().await;
    // b's own edit of a stock page stays b's; a's copy gets only the library's change.
    let page = |w: &WorkspaceHome| w.sites().join("home/index.md");
    let original = std::fs::read_to_string(page(&f.b)).unwrap();
    std::fs::write(page(&f.b), format!("b's line\n{original}")).unwrap();
    let library_page = f.home.library().join("home/index.md");
    std::fs::write(&library_page, format!("{original}\nlibrary line\n")).unwrap();
    Library::of(&f.home).record("edit").unwrap();
    update_home(&f.home, false).await.unwrap();
    let a_page = std::fs::read_to_string(page(&f.a)).unwrap();
    let b_page = std::fs::read_to_string(page(&f.b)).unwrap();
    assert!(a_page.contains("library line") && !a_page.contains("b's line"));
    assert!(b_page.contains("library line") && b_page.contains("b's line"));
}

#[tokio::test]
async fn publishing_a_site_never_removes_library_files_outside_its_definition() {
    let f = fixture().await;
    // a adds an agent of its own and publishes `agents` (installed from the library's stock).
    std::fs::write(
        f.a.sites().join("agents/printer.md"),
        "---\ntitle: printer\ntype: agent\nname: printer\ntools: [open]\n---\nYou print.\n",
    )
    .unwrap();
    std::fs::write(
        f.a.sites().join("agents/index.md"),
        std::fs::read_to_string(f.a.sites().join("agents/index.md")).unwrap() + "\nEdited.\n",
    )
    .unwrap();
    publish(&f.a, "agents").unwrap();
    // The stock agents stay in the library, and in b after its update.
    for agent in ["default.md", "orchestrator.md"] {
        assert!(
            f.home.library().join("agents").join(agent).is_file(),
            "library lost {agent}"
        );
    }
    // The stock `agents` lists `library: ["*.md"]`: a's new agent is part of the definition.
    assert!(f.home.library().join("agents/printer.md").is_file());
    let b = WorkspaceLibrary::of(&f.b).unwrap();
    b.update(false, &b.approvals().unwrap()).await.unwrap();
    assert!(
        f.b.sites().join("agents/default.md").is_file(),
        "b lost its default agent"
    );
    assert!(f.b.sites().join("agents/printer.md").is_file());
}

#[tokio::test]
async fn a_definition_file_removed_in_the_workspace_leaves_the_library_on_publish() {
    let f = fixture().await;
    publish(&f.a, "cad").unwrap();
    // A library file the definition doesn't cover, added by hand.
    std::fs::write(f.home.library().join("cad/README.txt"), "by hand\n").unwrap();
    Library::of(&f.home).record("by hand").unwrap();
    let a = WorkspaceLibrary::of(&f.a).unwrap();
    a.update(false, &a.approvals().unwrap()).await.unwrap();
    std::fs::remove_file(f.a.sites().join("cad/tools/slice.py")).unwrap();
    let library = WorkspaceLibrary::of(&f.a).unwrap();
    let plan = library.plan_publish(&site("cad"), None).unwrap();
    assert!(
        plan.describe().contains("README.txt"),
        "{}",
        plan.describe()
    );
    library.publish(&site("cad"), None, &plan.digest).unwrap();
    assert!(
        !f.home.library().join("cad/tools").exists(),
        "the emptied folder goes too"
    );
    assert!(f.home.library().join("cad/README.txt").is_file());
}

#[tokio::test]
async fn a_file_deleted_in_a_workspace_stays_deleted_on_update() {
    let f = fixture().await;
    publish(&f.a, "cad").unwrap();
    let b = WorkspaceLibrary::of(&f.b).unwrap();
    b.install(&site("cad"), None, &b.approvals().unwrap())
        .await
        .unwrap();
    std::fs::remove_file(f.b.sites().join("cad/tools/slice.py")).unwrap();
    std::fs::write(f.a.sites().join("cad/tools/slice.py"), "# Slice v2\n").unwrap();
    publish(&f.a, "cad").unwrap();
    let items = b.update(false, &b.approvals().unwrap()).await.unwrap();
    assert!(items.is_empty(), "{items:?}");
    assert!(!f.b.sites().join("cad/tools/slice.py").exists());
}

#[tokio::test]
async fn a_worker_can_not_publish() {
    use std::sync::Arc;

    use composer_protocol::{SessionInfo, TurnState};
    use composer_server::Server;
    use composer_server::agent::AgentDefinition;
    use composer_server::tools::ToolContext;
    use tokio_util::sync::CancellationToken;

    let f = fixture().await;
    let config = f.a.load_config().unwrap();
    let provider = Arc::new(support::MockProvider::new(vec![]));
    let server = Server::new(f.a.clone(), config, provider).unwrap();
    let worker = server
        .sessions
        .create_with_info(SessionInfo {
            id: "wk1".into(),
            agent: "default".into(),
            project: None,
            site: None,
            ticket: Some("0001".into()),
            parent: None,
            worker: None,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            model: None,
            preview: None,
            updated: chrono::Utc::now().to_rfc3339(),
        })
        .unwrap();
    // Publishing is home's verb: the worker is inside it and still refused.
    let site = Arc::new(
        composer_server::agent::Site::load(&server.web, "home")
            .map_err(|e| panic!("{e}"))
            .unwrap(),
    );
    let context = ToolContext {
        server: server.clone(),
        session: worker,
        agent: Arc::new(AgentDefinition::load(&server.web, "default").unwrap()),
        site: Some(site),
        cancel: CancellationToken::new(),
    };
    // A worker that got through would wait for an approval forever: fail instead of hanging.
    let call = server.tools.call_value(
        &context,
        "publish_site",
        serde_json::json!({ "site": "cad" }),
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), call)
        .await
        .expect("the worker's publish waited for an approval");
    assert!(
        result.is_error && result.text.contains("workers cannot publish"),
        "{}",
        result.text
    );
    assert!(
        server.pending_approvals.all().is_empty(),
        "nothing was asked"
    );
    assert!(!f.home.library().join("cad").exists());
}
