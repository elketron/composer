//! Nothing an agent (or a recipe, worker or MCP server it starts) does in one workspace reaches
//! another (ticket 0148). Each test here failed before its fix.

mod support;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use composer_server::actions::{Caller, Justfile};
use composer_server::worker::ParentLink;
use composer_server::{
    Config, Home, Host, McpServerConfig, Server, WorkspaceName, init_home, init_workspace,
};
use serde_json::{Value, json};
use support::MockProvider;

/// A home with `default` and `b` served by one host, and a projects folder (`x`, `y`, `z`, each
/// with a page that names it) that both use as their projects directory.
struct Two {
    _dir: tempfile::TempDir,
    projects: PathBuf,
    host: Arc<Host>,
    addr: SocketAddr,
}

impl Two {
    /// `prepare` runs on the home before the servers start; `configure` on each workspace's config.
    async fn start(
        prepare: impl Fn(&Home, &Path),
        configure: impl Fn(&WorkspaceName, &mut Config),
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects");
        for d in ["x", "y", "z"] {
            std::fs::create_dir_all(projects.join(d)).unwrap();
            std::fs::write(
                projects.join(d).join("note.md"),
                format!("# Okapi {d}\n\nokapi notes of project {d}\n"),
            )
            .unwrap();
        }
        let home = Home::new(dir.path().join("home"));
        init_home(&home).await.unwrap();
        init_workspace(&home.workspace(&WorkspaceName::new("b").unwrap()))
            .await
            .unwrap();
        prepare(&home, &projects);
        let mut servers = BTreeMap::new();
        for name in home.workspace_names().unwrap() {
            let workspace = home.workspace(&name);
            let mut config = workspace.load_config().unwrap();
            config.projects.dir = projects.display().to_string();
            configure(&name, &mut config);
            let provider = Arc::new(MockProvider::new(vec![]));
            servers.insert(name, Server::new(workspace, config, provider).unwrap());
        }
        let host = Arc::new(Host::new(home, servers).unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serving = host.clone();
        tokio::spawn(async move { serving.serve(listener).await });
        let two = Self {
            _dir: dir,
            projects,
            host,
            addr,
        };
        for _ in 0..100 {
            if two.a().parent_url().is_some() && two.b().parent_url().is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        two
    }

    fn a(&self) -> &Arc<Server> {
        self.host
            .workspace(&WorkspaceName::default_workspace())
            .unwrap()
    }

    fn b(&self) -> &Arc<Server> {
        self.host
            .workspace(&WorkspaceName::new("b").unwrap())
            .unwrap()
    }

    fn project(&self, name: &str) -> String {
        self.projects.join(name).display().to_string()
    }

    /// GET `path` with `token`: the status and the body.
    async fn get(&self, path: &str, token: &str) -> (u16, String) {
        let response = reqwest::Client::new()
            .get(format!("http://{}{path}", self.addr))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }

    /// The page URLs `server`'s search finds for `query`, over its API.
    async fn search(&self, server: &Server, query: &str) -> Vec<String> {
        let path = format!("{}/api/search?q={query}", server.route_base());
        let (status, body) = self.get(&path, &server.token).await;
        assert_eq!(status, 200, "{body}");
        let hits: Value = serde_json::from_str(&body).unwrap();
        hits.as_array()
            .unwrap()
            .iter()
            .map(|h| h["url"].as_str().unwrap().to_string())
            .collect()
    }
}

fn has(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .is_ok()
}

#[tokio::test]
async fn an_attachment_id_cannot_climb_into_another_workspace() {
    let two = Two::start(|_, _| {}, |_, _| {}).await;
    let (a, b) = (two.a(), two.b());
    let session = b.sessions.create("default", None).unwrap();
    let saved = composer_server::attachments::save(
        &b.home,
        &b.config().attachments,
        session.id(),
        "secret.txt",
        b"b's secret",
    )
    .unwrap();

    let own = format!(
        "/w/b/api/sessions/{}/attachments/{}",
        session.id(),
        saved.file
    );
    let (status, body) = two.get(&own, &b.token).await;
    assert_eq!((status, body.as_str()), (200, "b's secret"));

    // a's token, b's session folder spelled relative to a's sessions folder.
    let climbing = format!(
        "/w/default/api/sessions/..%2F..%2F..%2Fb%2Fstate%2Fsessions%2F{}/attachments/{}",
        session.id(),
        saved.file
    );
    let (status, body) = two.get(&climbing, &a.token).await;
    assert_eq!(status, 404, "{body}");
    // The same climb through the model's attachment path.
    let error = composer_server::attachments::resolve(
        &a.home,
        &format!("../../../b/state/sessions/{}", session.id()),
        &saved,
    )
    .unwrap_err();
    assert!(error.contains("no attachment"), "{error}");
}

#[tokio::test]
async fn search_never_shows_pages_of_another_workspaces_project() {
    // b's config names x, so x is b's before anything is indexed; a links x, y and z into its web.
    let two = Two::start(
        |home, projects| {
            let a = home.workspace(&WorkspaceName::default_workspace());
            for d in ["x", "y", "z"] {
                std::os::unix::fs::symlink(projects.join(d), a.sites().join(format!("mnt-{d}")))
                    .unwrap();
            }
            let b = home.workspace(&WorkspaceName::new("b").unwrap());
            let x = projects.join("x").display().to_string();
            std::fs::write(
                b.config_file(),
                format!("[projects.\"{x}\"]\nexternal_harnesses = []\n"),
            )
            .unwrap();
        },
        |_, _| {},
    )
    .await;
    let (a, b) = (two.a(), two.b());
    a.use_project(&two.project("y")).unwrap();

    let found = two.search(a, "okapi").await;
    assert!(found.iter().any(|u| u.contains("mnt-y")), "{found:?}");
    assert!(found.iter().any(|u| u.contains("mnt-z")), "{found:?}");
    assert!(
        !found.iter().any(|u| u.contains("mnt-x")),
        "x is b's: {found:?}"
    );

    // Once b claims z, a's index lets go of it too.
    b.use_project(&two.project("z")).unwrap();
    let found = two.search(a, "okapi").await;
    assert!(
        !found.iter().any(|u| u.contains("mnt-z")),
        "z is b's now: {found:?}"
    );
    assert!(found.iter().any(|u| u.contains("mnt-y")), "{found:?}");
}

#[tokio::test]
async fn a_workers_server_knows_what_other_workspaces_own() {
    let two = Two::start(|_, _| {}, |_, _| {}).await;
    let (a, b) = (two.a(), two.b());
    b.use_project(&two.project("y")).unwrap();
    std::os::unix::fs::symlink(two.projects.join("y"), a.web.root().join("mnt-y")).unwrap();

    // A worker process of a: its own server, connected to a's.
    let link = ParentLink::open(&a.parent_url().unwrap(), &a.token)
        .await
        .unwrap();
    let worker = Server::new_worker(
        a.home.clone(),
        a.config().as_ref().clone(),
        Arc::new(MockProvider::new(vec![])),
        link,
    )
    .unwrap();
    let error = worker.use_project(&two.project("y")).unwrap_err();
    assert!(error.contains("workspace `b`"), "{error}");
    assert!(!worker.web.allows(&a.web.root().join("mnt-y/note.md")));
    worker.use_project(&two.project("x")).unwrap();
}

#[tokio::test]
async fn no_project_or_link_reaches_into_another_workspaces_folder() {
    // A projects directory that holds the whole home: b's folder is in reach by name.
    let two = Two::start(|_, _| {}, |_, _| {}).await;
    let (a, b) = (two.a(), two.b());
    let b_sites = b.web.root().display().to_string();
    let error = a.use_project(&b_sites).unwrap_err();
    assert!(error.contains("workspace `b`"), "{error}");
    assert!(a.see_project(&b.home.root().display().to_string()).is_err());
    // The home root holds every workspace (and the master token).
    assert!(
        a.use_project(&a.home.home_root().display().to_string())
            .is_err()
    );
    // b may use folders of its own.
    let own = b.home.root().join("work");
    std::fs::create_dir_all(&own).unwrap();
    b.use_project(&own.display().to_string()).unwrap();

    // A link in a's web to b's sites is refused, even with the projects directory allowing it.
    std::os::unix::fs::symlink(b.web.root(), a.web.root().join("mnt-b")).unwrap();
    assert!(!a.web.allows(&a.web.root().join("mnt-b/memory/index.md")));
    assert!(a.foreign_path(b.home.root()));
    assert!(!a.foreign_path(&two.projects.join("x")));
}

#[tokio::test]
async fn recipes_run_in_their_own_workspace() {
    if !has("just") {
        return;
    }
    let two = Two::start(|_, _| {}, |_, _| {}).await;
    let b = two.b();
    let site = b.web.root().join("probe");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(site.join("index.md"), "# Probe\n").unwrap();
    std::fs::write(
        site.join("justfile"),
        "ws:\n    @echo \"ws=$COMPOSER_WORKSPACE home=$COMPOSER_WORKSPACE_HOME\"\n",
    )
    .unwrap();
    let justfile = Justfile::load(&site).await.unwrap();
    b.approvals
        .approve("probe", "ws", &justfile.hash("ws"), Caller::App)
        .unwrap();

    let response = reqwest::Client::new()
        .post(format!("http://{}/w/b/api/actions/probe/ws", two.addr))
        .bearer_auth(&b.token)
        .json(&json!({ "args": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let output: Value = response.json().await.unwrap();
    let output = output["output"].as_str().unwrap();
    assert!(
        output.contains(&format!("ws=b home={}", b.home.root().display())),
        "{output}"
    );
}

/// A stdio MCP server whose one tool is named after the workspace it was started in.
const ENV_SERVER: &str = r#"
import json, os, sys
for line in sys.stdin:
    m = json.loads(line)
    mid, p = m.get("id"), m.get("params") or {}
    if mid is None:
        continue
    if m.get("method") == "initialize":
        r = {"protocolVersion": p["protocolVersion"], "capabilities": {"tools": {}},
             "serverInfo": {"name": "env", "version": "1"}}
    else:
        r = {"tools": [{"name": "in_" + os.environ.get("COMPOSER_WORKSPACE", "none")},
                       {"name": "named_" + os.environ.get("NAMED", "none")}]}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": r}) + "\n")
    sys.stdout.flush()
"#;

#[tokio::test]
async fn mcp_servers_start_in_their_own_workspace() {
    if !has("python3") {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let script = scratch.path().join("env_mcp.py");
    std::fs::write(&script, ENV_SERVER).unwrap();
    let two = Two::start(
        |_, _| {},
        |_, config| {
            config.mcp.insert(
                "env".into(),
                McpServerConfig {
                    command: "python3".into(),
                    args: vec![script.display().to_string()],
                    env: BTreeMap::from([("NAMED".into(), "${COMPOSER_WORKSPACE}".into())]),
                    enabled: true,
                    ..Default::default()
                },
            );
        },
    )
    .await;
    for (server, name) in [(two.b(), "b"), (two.a(), "default")] {
        let listed = server.mcp().list(&[]).await;
        assert_eq!(
            listed[0].tools,
            [format!("env__in_{name}"), format!("env__named_{name}")],
            "{:?}",
            listed[0].error
        );
    }
}
