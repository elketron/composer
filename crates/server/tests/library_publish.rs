//! Publishing a workspace site to the library (ticket 0153): only its definition goes, only what
//! was approved, never over a library copy this workspace hasn't seen; agents can only ask.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use composer_protocol::{ApprovalDecision, ServerMessage};
use composer_server::agent::AgentDefinition;
use composer_server::library::{Library, SiteName, WorkspaceLibrary};
use composer_server::tools::ToolContext;
use composer_server::{Host, Server, WorkspaceHome, WorkspaceName, init_home, init_workspace};
use serde_json::json;
use support::MockProvider;
use tokio_util::sync::CancellationToken;

struct Fixture {
    _dir: tempfile::TempDir,
    home: composer_server::Home,
    a: WorkspaceHome,
    b: WorkspaceHome,
}

/// Workspaces `default` (a) and `b`; a has a pipeline site `cad` with a ui, a stage, a `library:`
/// extra, and content: a work, tickets, a tmp folder, node_modules.
async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let home = composer_server::Home::new(dir.path());
    init_home(&home).await.unwrap();
    let b = home.workspace(&WorkspaceName::new("b").unwrap());
    init_workspace(&b).await.unwrap();
    let a = home.workspace(&WorkspaceName::default_workspace());
    let cad = a.sites().join("cad");
    let write = |rel: &str, text: &str| {
        let path = cad.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        "index.md",
        "---\ntitle: CAD\ntype: pipeline\nlibrary: [tools/]\n---\n# CAD\n",
    );
    write("justfile", "build:\n    @echo built\n");
    write("ui/src/main.ts", "console.log('cad')\n");
    write("ui/node_modules/dep/index.js", "x");
    write("stages/01_plan.md", "# Plan\n");
    write("tools/slice.py", "print('slice')\n");
    write(
        "moonlander/index.md",
        "---\ntype: work\n---\n# The user's work\n",
    );
    write("tickets/0001.md", "# A ticket\n");
    write("tmp/render.png", "png");
    Fixture {
        _dir: dir,
        home,
        a,
        b,
    }
}

fn site(name: &str) -> SiteName {
    SiteName::new(name).unwrap()
}

fn library_of(workspace: &WorkspaceHome) -> WorkspaceLibrary {
    WorkspaceLibrary::of(workspace).unwrap()
}

fn publish(workspace: &WorkspaceHome, name: &str, as_name: Option<&str>) -> Result<(), String> {
    let library = library_of(workspace);
    let as_name = as_name.map(site);
    let plan = library.plan_publish(&site(name), as_name.as_ref())?;
    library
        .publish(&site(name), as_name.as_ref(), &plan.digest)
        .map(|_| ())
}

fn edit(workspace: &WorkspaceHome, rel: &str, text: &str) {
    std::fs::write(workspace.sites().join(rel), text).unwrap();
}

#[tokio::test]
async fn only_the_definition_is_published_and_others_can_install_it() {
    let f = fixture().await;
    let plan = library_of(&f.a).plan_publish(&site("cad"), None).unwrap();
    assert!(plan.new_site);
    let files: Vec<String> = plan.files.iter().map(|p| p.display().to_string()).collect();
    assert_eq!(
        files,
        [
            "index.md",
            "justfile",
            "stages/01_plan.md",
            "tools/slice.py",
            "ui/src/main.ts"
        ]
    );
    let left_out: Vec<String> = plan
        .left_out
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    assert_eq!(left_out, ["moonlander", "tickets", "tmp"]);
    assert!(
        plan.describe()
            .contains("Not published (content, not definition): moonlander")
    );

    publish(&f.a, "cad", None).unwrap();
    let in_library = f.home.library().join("cad");
    assert!(in_library.join("tools/slice.py").is_file());
    for content in ["moonlander", "tickets", "tmp", "ui/node_modules"] {
        assert!(!in_library.join(content).exists(), "{content} leaked");
    }
    let log = Library::of(&f.home).head().unwrap().unwrap();
    // a's copy is now installed from it; b installs it.
    let installed = library_of(&f.a).installed().unwrap();
    assert_eq!(installed.get(&site("cad")).unwrap().commit, log);
    let b = library_of(&f.b);
    b.install(&site("cad"), None, &b.approvals().unwrap())
        .await
        .unwrap();
    assert!(f.b.sites().join("cad/stages/01_plan.md").is_file());
    assert!(!f.b.sites().join("cad/moonlander").exists());

    // A change published from a reaches b on update.
    edit(&f.a, "cad/stages/01_plan.md", "# Plan v2\n");
    let plan = library_of(&f.a).plan_publish(&site("cad"), None).unwrap();
    assert_eq!(
        plan.changes,
        [(
            std::path::PathBuf::from("stages/01_plan.md"),
            composer_server::library::PublishChange::Changed
        )]
    );
    assert!(plan.diff.contains("+# Plan v2"), "{}", plan.diff);
    publish(&f.a, "cad", None).unwrap();
    b.update(false, &b.approvals().unwrap()).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(f.b.sites().join("cad/stages/01_plan.md")).unwrap(),
        "# Plan v2\n"
    );
    // Nothing left to publish.
    let error = publish(&f.a, "cad", None).unwrap_err();
    assert!(error.contains("nothing to publish"), "{error}");
}

#[tokio::test]
async fn never_over_a_library_copy_this_workspace_has_not_seen() {
    let f = fixture().await;
    publish(&f.a, "cad", None).unwrap();
    let b = library_of(&f.b);
    b.install(&site("cad"), None, &b.approvals().unwrap())
        .await
        .unwrap();
    // b publishes a change; a, behind, may not publish over it.
    edit(&f.b, "cad/justfile", "build:\n    @echo b's build\n");
    publish(&f.b, "cad", None).unwrap();
    edit(&f.a, "cad/stages/01_plan.md", "# a's plan\n");
    let error = publish(&f.a, "cad", None).unwrap_err();
    assert!(error.contains("composer library update"), "{error}");
    // After updating, a may.
    let a = library_of(&f.a);
    a.update(false, &a.approvals().unwrap()).await.unwrap();
    publish(&f.a, "cad", None).unwrap();

    // A site of a's own named like a library site it didn't install: refused, unless renamed.
    std::fs::create_dir_all(f.b.sites().join("other")).unwrap();
    edit(&f.b, "other/index.md", "# b's own\n");
    std::fs::rename(f.b.sites().join("other"), f.b.sites().join("cad2")).unwrap();
    publish(&f.b, "cad2", None).unwrap();
    std::fs::create_dir_all(f.a.sites().join("cad2")).unwrap();
    edit(&f.a, "cad2/index.md", "# a's own cad2\n");
    let error = publish(&f.a, "cad2", None).unwrap_err();
    assert!(error.contains("wasn't installed from it"), "{error}");
    publish(&f.a, "cad2", Some("cad2-a")).unwrap();
    assert!(f.home.library().join("cad2-a/index.md").is_file());
    assert_eq!(
        std::fs::read_to_string(f.home.library().join("cad2/index.md")).unwrap(),
        "# b's own\n"
    );
}

#[tokio::test]
async fn only_what_was_approved_is_published() {
    let f = fixture().await;
    let library = library_of(&f.a);
    let plan = library.plan_publish(&site("cad"), None).unwrap();
    edit(
        &f.a,
        "cad/justfile",
        "build:\n    @echo changed after approval\n",
    );
    let error = library
        .publish(&site("cad"), None, &plan.digest)
        .unwrap_err();
    assert!(
        error.contains("changed since the publish was approved"),
        "{error}"
    );
    assert!(!f.home.library().join("cad").exists());
}

#[tokio::test]
async fn an_agent_can_only_ask_and_the_user_decides_each_time() {
    let f = fixture().await;
    let mut servers = BTreeMap::new();
    for name in f.home.workspace_names().unwrap() {
        let workspace = f.home.workspace(&name);
        let config = workspace.load_config().unwrap();
        let provider = Arc::new(MockProvider::new(vec![]));
        servers.insert(name, Server::new(workspace, config, provider).unwrap());
    }
    let host = Host::new(f.home.clone(), servers).unwrap();
    let server = host
        .workspace(&WorkspaceName::default_workspace())
        .unwrap()
        .clone();
    let session = server.sessions.create("default", None).unwrap();
    let site = Arc::new(
        composer_server::agent::Site::load(&server.web, "home")
            .map_err(|e| panic!("{e}"))
            .unwrap(),
    );
    let context = ToolContext {
        server: server.clone(),
        session,
        agent: Arc::new(AgentDefinition::load(&server.web, "default").unwrap()),
        site: Some(site),
        cancel: CancellationToken::new(),
    };
    for decision in [ApprovalDecision::Deny, ApprovalDecision::Always] {
        let call = {
            let context = context.clone();
            tokio::spawn(async move {
                context
                    .server
                    .tools
                    .call_value(&context, "publish_site", json!({ "site": "cad" }))
                    .await
            })
        };
        let request = loop {
            let pending = server.pending_approvals.all();
            if let Some(ServerMessage::ApprovalRequest { request, body, .. }) = pending.first() {
                assert!(
                    body.contains("ui/src/main.ts") && body.contains("moonlander"),
                    "{body}"
                );
                break request.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert!(server.pending_approvals.resolve(&request, decision));
        let result = call.await.unwrap();
        match decision {
            ApprovalDecision::Deny => {
                assert!(result.is_error, "{}", result.text);
                assert!(!f.home.library().join("cad").exists(), "nothing on a no");
            }
            _ => {
                assert!(!result.is_error, "{}", result.text);
                assert!(f.home.library().join("cad/index.md").is_file());
            }
        }
    }
    // `always` was one yes: the next publish asks again.
    edit(&f.a, "cad/stages/01_plan.md", "# Plan v3\n");
    let call = {
        let context = context.clone();
        tokio::spawn(async move {
            context
                .server
                .tools
                .call_value(&context, "publish_site", json!({ "site": "cad" }))
                .await
        })
    };
    let asked = tokio::time::timeout(Duration::from_secs(5), async {
        while server.pending_approvals.all().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(asked.is_ok(), "publishing asks every time");
    context.cancel.cancel();
    let _ = call.await;
}
