//! The two-workspace boundary suite (ticket 0149). Workspaces `default` (A) and `b` (B) each
//! have a project, pages, a session with an attachment, a pending approval, a worker session, a
//! ticket with its worktree, a watchable page, a reminder, a site with recipes and a built app.
//! From A, every route, command, tool, recipe, search and event is tried against B's things and
//! must fail (404, refused, not found), while the same attempt against A's own things succeeds.

mod support;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDateTime;
use composer_core::SiteUrl;
use composer_protocol::{
    ApprovalDecision, ApprovalKind, ChangedBy, ClientCommand, ClientRequest, CommandResult,
    ServerMessage, SessionInfo, TurnState, WorkerState,
};
use composer_server::actions::{Caller, Justfile};
use composer_server::agent::AgentDefinition;
use composer_server::reminders::check_reminders;
use composer_server::tools::ToolContext;
use composer_server::{Home, Host, Server, WorkspaceName, init_home, init_workspace};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use support::MockProvider;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

/// One workspace's things, by the names a client or an agent would use for them.
struct Side {
    server: Arc<Server>,
    /// `default` or `b`, as in `/w/<name>`.
    name: &'static str,
    /// A site only this workspace has (pages, a recipe, a built app, a data file).
    site: String,
    /// A word only this workspace's pages contain.
    word: String,
    project: String,
    session: String,
    attachment: String,
    approval: String,
    worker: String,
    ticket: String,
}

impl Side {
    fn base(&self) -> String {
        format!("/w/{}", self.name)
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    addr: SocketAddr,
    a: Side,
    b: Side,
    _host: Arc<Host>,
}

const AGENT_ALL: &str = "---\ntitle: all\ntype: agent\nname: all\ntools: [open, search, list_sites, \
write, edit, actions, run, set_project, spawn_worker, merge_ticket, continue_worker, stop_worker, \
watch]\nsites: all\n---\nEvery tool.\n";

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A git project with `pic.png`, and ticket `ticket`'s worktree (one commit ahead) in `server`'s
/// `state/worktrees`, with its ticket page.
fn project_and_ticket(server: &Server, project: &Path, ticket: &str) {
    std::fs::create_dir_all(project).unwrap();
    git(project, &["init", "-q", "-b", "main"]);
    std::fs::write(project.join("README.md"), "# project\n").unwrap();
    // A 1x1 PNG.
    let png = base64_png();
    std::fs::write(project.join("pic.png"), png).unwrap();
    git(project, &["add", "."]);
    git(project, &["commit", "-q", "-m", "start"]);
    let worktree = server.home.state().join("worktrees").join(ticket);
    std::fs::create_dir_all(worktree.parent().unwrap()).unwrap();
    git(
        project,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("ticket/{ticket}"),
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::write(worktree.join("change.txt"), "the ticket's change\n").unwrap();
    git(&worktree, &["add", "."]);
    git(&worktree, &["commit", "-q", "-m", "change"]);
    let url = SiteUrl::new("orchestration", &format!("tickets/{ticket}.md")).unwrap();
    server
        .write_page(
            &url,
            &format!(
                "---\ntitle: Ticket {ticket}\ntype: ticket\nid: \"{ticket}\"\nstatus: done\nagent: worker\nproject: {}\n---\n# Ticket {ticket}\n",
                project.display()
            ),
            ChangedBy::External,
        )
        .unwrap();
}

fn base64_png() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
        .unwrap()
}

async fn side(
    server: Arc<Server>,
    name: &'static str,
    projects: &Path,
    ticket: &str,
    word: &str,
) -> Side {
    let site = format!("only-{name}");
    let web = server.web.root().to_path_buf();
    for dir in [site.as_str(), "plans/reminders", "orchestration/tickets"] {
        std::fs::create_dir_all(web.join(dir)).unwrap();
    }
    // The agent every tool attempt runs as.
    server
        .write_page(
            &SiteUrl::new("agents", "all.md").unwrap(),
            AGENT_ALL,
            ChangedBy::External,
        )
        .unwrap();
    // Pages, a recipe, a built app and a data file.
    server
        .write_page(
            &SiteUrl::new(&site, "index.md").unwrap(),
            &format!("---\ntitle: Only {name}\n---\n# Only {name}\n\nThe {word} lives here.\n"),
            ChangedBy::External,
        )
        .unwrap();
    std::fs::write(
        web.join(&site).join("justfile"),
        format!("hello:\n    @echo hello-from-{name}\n"),
    )
    .unwrap();
    std::fs::create_dir_all(web.join(&site).join("ui/dist")).unwrap();
    std::fs::write(
        web.join(&site).join("ui/dist/index.html"),
        format!("<!doctype html><title>app of {name}</title>"),
    )
    .unwrap();
    std::fs::write(
        web.join(&site).join("data.json"),
        format!("{{\"of\":\"{name}\"}}"),
    )
    .unwrap();
    if has("just") {
        let justfile = Justfile::load(&web.join(&site)).await.unwrap();
        for caller in [Caller::Agent, Caller::App] {
            server
                .approvals
                .approve(&site, "hello", &justfile.hash("hello"), caller)
                .unwrap();
        }
    }
    // A reminder, due at 2099-01-01 09:00.
    server
        .write_page(
            &SiteUrl::new("plans", &format!("reminders/{name}.md")).unwrap(),
            &format!("---\ntitle: Reminder of {name}\ntype: reminder\ndue: 2099-01-01\ntime: \"09:00\"\nstatus: open\n---\n# Reminder\n"),
            ChangedBy::External,
        )
        .unwrap();
    // A project with a ticket in its worktree, used (claimed) by this workspace.
    let project_dir = projects.join(format!("p-{name}"));
    project_and_ticket(&server, &project_dir, ticket);
    let project = server
        .use_project(&project_dir.display().to_string())
        .unwrap();
    // A session in it, with an attachment.
    let session = server
        .sessions
        .create("all", Some(project.clone()))
        .unwrap();
    let attachment = composer_server::attachments::save(
        &server.home,
        &server.config().attachments,
        session.id(),
        "notes.txt",
        format!("notes of {name}").as_bytes(),
    )
    .unwrap();
    // A pending approval of that session.
    let approval = format!("approval-{name}");
    let _answer = server.pending_approvals.add(
        &approval,
        ServerMessage::ApprovalRequest {
            request: approval.clone(),
            session: session.id().to_string(),
            kind: ApprovalKind::default(),
            title: String::new(),
            site: site.clone(),
            recipe: "hello".into(),
            args: vec![],
            body: String::new(),
            hash: String::new(),
            rule: None,
        },
    );
    std::mem::forget(_answer);
    // A paused worker on the ticket.
    let worker = format!("wk{name}");
    server
        .sessions
        .create_with_info(SessionInfo {
            id: worker.clone(),
            agent: "worker".into(),
            project: Some(project.clone()),
            ticket: Some(ticket.into()),
            parent: Some(session.id().to_string()),
            worker: Some(WorkerState::Paused),
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            model: None,
            preview: None,
            updated: chrono::Utc::now().to_rfc3339(),
        })
        .unwrap();
    Side {
        server,
        name,
        site,
        word: word.into(),
        project,
        session: session.id().to_string(),
        attachment: attachment.file,
        approval,
        worker,
        ticket: ticket.into(),
    }
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join("projects");
    let home = Home::new(dir.path().join("home"));
    init_home(&home).await.unwrap();
    init_workspace(&home.workspace(&WorkspaceName::new("b").unwrap()))
        .await
        .unwrap();
    let mut servers = BTreeMap::new();
    for name in home.workspace_names().unwrap() {
        let workspace = home.workspace(&name);
        let mut config = workspace.load_config().unwrap();
        config.projects.dir = projects.display().to_string();
        // A launcher entry on its own project and one on the other workspace's.
        let other = if name.as_str() == "default" {
            "b"
        } else {
            "default"
        };
        config.launcher = [("own", name.as_str()), ("theirs", other)]
            .into_iter()
            .map(|(entry, owner)| composer_server::LauncherEntry {
                name: entry.into(),
                command: "true".into(),
                folder: projects.join(format!("p-{owner}")).display().to_string(),
                args: vec![],
            })
            .collect();
        let provider = Arc::new(MockProvider::new(vec![]));
        servers.insert(name, Server::new(workspace, config, provider).unwrap());
    }
    let host = Arc::new(Host::new(home, servers).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = host.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    let get = |name: &str| {
        host.workspace(&WorkspaceName::new(name).unwrap())
            .unwrap()
            .clone()
    };
    for _ in 0..200 {
        if get("default").parent_url().is_some() && get("b").parent_url().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let a = side(get("default"), "default", &projects, "0001", "quokka").await;
    let b = side(get("b"), "b", &projects, "0002", "zebu").await;
    Fixture {
        _dir: dir,
        addr,
        a,
        b,
        _host: host,
    }
}

fn has(program: &str) -> bool {
    Command::new(program).arg("--version").output().is_ok()
}

// ---- HTTP ----------------------------------------------------------------------------------

async fn http(
    addr: SocketAddr,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> (u16, String) {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let url = format!("http://{addr}{path}");
    let request = match method {
        "GET" => client.get(url),
        "PUT" => client.put(url),
        _ => client.post(url),
    };
    let request = match body {
        Some(body) => request.json(&body),
        None => request,
    };
    let response = request.bearer_auth(token).send().await.unwrap();
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

/// Every route of a workspace, filled in with `side`'s things.
fn routes(side: &Side) -> Vec<(&'static str, String, Option<Value>)> {
    let base = side.base();
    let (site, session, file) = (&side.site, &side.session, &side.attachment);
    vec![
        ("GET", format!("{base}/api/info"), None),
        ("GET", format!("{base}/api/sites"), None),
        ("GET", format!("{base}/api/agents"), None),
        ("GET", format!("{base}/api/projects"), None),
        ("GET", format!("{base}/api/search?q={}", side.word), None),
        ("GET", format!("{base}/api/pages/{site}"), None),
        ("GET", format!("{base}/api/pages/{site}/index.md"), None),
        (
            "PUT",
            format!("{base}/api/pages/{site}/new.md"),
            Some(json!({ "content": "# new\n" })),
        ),
        (
            "POST",
            format!("{base}/api/sessions/{session}/messages"),
            Some(json!({ "text": "hi" })),
        ),
        (
            "GET",
            format!("{base}/api/sessions/{session}/attachments/{file}"),
            None,
        ),
        (
            "GET",
            format!("{base}/api/sessions/{session}/project/diff"),
            None,
        ),
        ("GET", format!("{base}/api/actions/{site}"), None),
        (
            "POST",
            format!("{base}/api/actions/{site}/hello"),
            Some(json!({ "args": [] })),
        ),
        ("GET", format!("{base}/api/launcher"), None),
        ("GET", format!("{base}/api/files/{site}/data.json"), None),
        ("GET", format!("{base}/{site}/"), None),
        ("GET", format!("{base}/{site}/index.html"), None),
    ]
}

#[tokio::test]
async fn a_token_opens_none_of_the_other_workspaces_routes() {
    let f = fixture().await;
    for (method, path, body) in routes(&f.b) {
        if path.contains("/api/actions/") && !has("just") {
            continue;
        }
        let (status, text) = http(f.addr, method, &path, &f.a.server.token, body.clone()).await;
        assert_eq!(status, 404, "A's token on {method} {path}: {text}");
        let (status, text) = http(f.addr, method, &path, &f.b.server.token, body).await;
        assert!(
            (200..300).contains(&status),
            "B's own token on {method} {path}: {status} {text}"
        );
    }
    // The WebSocket too.
    let ws = format!("ws://{}/w/b/ws?token={}", f.addr, f.a.server.token);
    assert!(tokio_tungstenite::connect_async(&ws).await.is_err());
    let ws = format!("ws://{}/w/b/ws?token={}", f.addr, f.b.server.token);
    assert!(tokio_tungstenite::connect_async(&ws).await.is_ok());
}

#[tokio::test]
async fn a_workspaces_routes_never_serve_the_others_things() {
    let f = fixture().await;
    let token = &f.a.server.token;
    // The same route shapes on A's own base, filled with A's things (served) and B's (not).
    let own = routes(&f.a);
    let others: Vec<_> = routes(&f.b)
        .into_iter()
        .map(|(m, p, b)| (m, p.replacen("/w/b/", "/w/default/", 1), b))
        .collect();
    for ((method, path, body), (_, attack, attack_body)) in own.into_iter().zip(others) {
        if path.contains("/api/actions/") && !has("just") {
            continue;
        }
        let (status, text) = http(f.addr, method, &path, token, body).await;
        assert!(
            (200..300).contains(&status),
            "A on its own {method} {path}: {status} {text}"
        );
        let (status, text) = http(f.addr, method, &attack, token, attack_body).await;
        let b_things = [
            f.b.word.as_str(),
            f.b.site.as_str(),
            f.b.project.as_str(),
            "notes of b",
            "hello-from-b",
            "app of b",
        ];
        if (200..300).contains(&status) {
            // A route that answers for any name (a listing, a search, a write) must answer
            // with A's things only.
            for thing in b_things {
                assert!(
                    !text.contains(thing) || method == "PUT",
                    "A on {method} {attack} showed B's `{thing}`: {text}"
                );
            }
        }
        // Whatever happened, B is untouched.
        assert!(
            !f.b.server
                .web
                .root()
                .join(&f.b.site)
                .join("new.md")
                .exists()
        );
    }
    // Spelling B's session folder relative to A's.
    let climb = format!(
        "/w/default/api/sessions/..%2F..%2F..%2Fb%2Fstate%2Fsessions%2F{}/attachments/{}",
        f.b.session, f.b.attachment
    );
    assert_eq!(http(f.addr, "GET", &climb, token, None).await.0, 404);
    let climb = format!(
        "/w/default/api/files/..%2F..%2Fb%2Fsites%2F{}/data.json",
        f.b.site
    );
    assert_ne!(http(f.addr, "GET", &climb, token, None).await.0, 200);
}

// ---- WebSocket commands ----------------------------------------------------------------------

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn socket(f: &Fixture, side: &Side) -> Socket {
    let url = format!(
        "ws://{}{}/ws?token={}",
        f.addr,
        side.base(),
        side.server.token
    );
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

/// Sends `command` and waits for its reply: the result, or the error message.
async fn command(
    socket: &mut Socket,
    id: u64,
    command: ClientCommand,
) -> Result<CommandResult, String> {
    let json = serde_json::to_string(&ClientRequest { id, command }).unwrap();
    socket.send(Message::Text(json.into())).await.unwrap();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await
            .expect("no reply")
            .unwrap()
            .unwrap();
        let Message::Text(text) = message else {
            continue;
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
}

/// The commands that name one of `side`'s things, as A's client would send them.
fn commands(a: &Side, side: &Side) -> Vec<(&'static str, ClientCommand)> {
    vec![
        (
            "session.attach",
            ClientCommand::SessionAttach {
                session: side.session.clone(),
                since: None,
            },
        ),
        (
            "session.history",
            ClientCommand::SessionHistory {
                session: side.session.clone(),
                before: None,
                messages: 10,
            },
        ),
        (
            "session.create in the project",
            ClientCommand::SessionCreate {
                agent: "all".into(),
                project: Some(side.project.clone()),
            },
        ),
        (
            "project.set",
            ClientCommand::ProjectSet {
                session: a.session.clone(),
                path: Some(side.project.clone()),
            },
        ),
        (
            "worker.hello",
            ClientCommand::WorkerHello {
                session: side.worker.clone(),
            },
        ),
        (
            "ticket.changes",
            ClientCommand::TicketChanges {
                ticket: side.ticket.clone(),
            },
        ),
        (
            "approval.respond",
            ClientCommand::ApprovalRespond {
                request: side.approval.clone(),
                decision: ApprovalDecision::Deny,
            },
        ),
    ]
}

#[tokio::test]
async fn commands_never_reach_the_other_workspaces_things() {
    let f = fixture().await;
    let mut a = socket(&f, &f.a).await;
    let mut id = 0;
    for ((what, own), (_, attack)) in commands(&f.a, &f.a).into_iter().zip(commands(&f.a, &f.b)) {
        id += 1;
        let result = command(&mut a, id, own).await;
        assert!(result.is_ok(), "A's {what} on its own: {result:?}");
        id += 1;
        let result = command(&mut a, id, attack).await;
        assert!(result.is_err(), "A's {what} on B's: {result:?}");
    }
    // B's approval is still open, its session still in its own project.
    assert_eq!(f.b.server.pending_approvals.all().len(), 1);
    assert_eq!(
        f.a.server
            .sessions
            .get(&f.a.session)
            .unwrap()
            .info()
            .project,
        Some(f.a.project.clone())
    );
}

/// Ids that climb from A's `state/worktrees` into B's.
fn climbing_ticket(b: &Side) -> String {
    format!("../../../b/state/worktrees/{}", b.ticket)
}

#[tokio::test]
async fn ticket_ids_never_climb_into_the_other_workspaces_worktrees() {
    let f = fixture().await;
    let mut a = socket(&f, &f.a).await;
    let climb = climbing_ticket(&f.b);
    let result = command(
        &mut a,
        1,
        ClientCommand::TicketChanges {
            ticket: climb.clone(),
        },
    )
    .await;
    assert!(result.is_err(), "A saw B's ticket changes: {result:?}");
    let result = command(
        &mut a,
        2,
        ClientCommand::TicketMerge {
            ticket: climb.clone(),
        },
    )
    .await;
    assert!(result.is_err(), "A merged B's ticket: {result:?}");
    let result = command(
        &mut a,
        3,
        ClientCommand::TicketDiscard {
            ticket: climb.clone(),
        },
    )
    .await;
    assert!(result.is_err(), "A discarded B's ticket: {result:?}");
    // The merge tool, as A's agent.
    let out = tool(&f.a, "merge_ticket", json!({ "ticket": climb })).await;
    assert!(out.1, "A's agent merged B's ticket: {}", out.0);
    // B's ticket branch is still there, unmerged.
    let worktree = f.b.server.home.state().join("worktrees").join(&f.b.ticket);
    assert!(worktree.join("change.txt").is_file());
    assert!(!Path::new(&f.b.project).join("change.txt").exists());
    // A's own ticket merges.
    let result = command(
        &mut a,
        4,
        ClientCommand::TicketMerge {
            ticket: f.a.ticket.clone(),
        },
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
}

// ---- Agent tools ------------------------------------------------------------------------------

/// Calls `name` as A's `all` agent in A's session: its output and whether it is an error.
async fn tool(a: &Side, name: &str, args: Value) -> (String, bool) {
    let context = ToolContext {
        server: a.server.clone(),
        session: a.server.sessions.get(&a.session).unwrap(),
        agent: Arc::new(AgentDefinition::load(&a.server.web, "all").unwrap()),
        cancel: CancellationToken::new(),
    };
    let result = a.server.tools.call_value(&context, name, args).await;
    (result.text, result.is_error)
}

/// Tool calls naming `side`'s things, and a piece of text only a success would show.
fn tool_calls(side: &Side) -> Vec<(&'static str, Value, String)> {
    let mut calls = vec![
        (
            "open",
            json!({ "url": format!("site://{}/index.md", side.site) }),
            side.word.clone(),
        ),
        (
            "open",
            json!({ "url": format!("{}/pic.png", side.project) }),
            "image".into(),
        ),
        ("search", json!({ "query": side.word }), side.site.clone()),
        ("list_sites", json!({}), side.site.clone()),
        (
            "set_project",
            json!({ "path": side.project }),
            String::new(),
        ),
    ];
    if has("just") {
        calls.push(("actions", json!({ "site": side.site }), "hello".into()));
        calls.push((
            "run",
            json!({ "site": side.site, "recipe": "hello" }),
            format!("hello-from-{}", side.name),
        ));
    }
    calls
}

#[tokio::test]
async fn agent_tools_never_reach_the_other_workspaces_things() {
    let f = fixture().await;
    for ((name, own, shows), (_, attack, b_shows)) in
        tool_calls(&f.a).into_iter().zip(tool_calls(&f.b))
    {
        let (out, error) = tool(&f.a, name, own.clone()).await;
        assert!(!error && out.contains(&shows), "A's {name} {own}: {out}");
        let (out, error) = tool(&f.a, name, attack.clone()).await;
        let reached = !error && (b_shows.is_empty() || out.contains(&b_shows));
        assert!(!reached, "A's {name} {attack} reached B: {out}");
    }
    // Set back to A's own project (the last own set_project did that); spawn and steer by B's
    // ticket find nothing in A.
    for (name, args) in [
        ("spawn_worker", json!({ "ticket": f.b.ticket })),
        (
            "stop_worker",
            json!({ "ticket": f.b.ticket, "message": "x" }),
        ),
        (
            "continue_worker",
            json!({ "ticket": f.b.ticket, "message": "x" }),
        ),
    ] {
        let (out, error) = tool(&f.a, name, args).await;
        assert!(error, "A's {name} on B's ticket: {out}");
    }
    // A write naming B's site is refused (A has no such site) and never lands in B.
    let url = format!("site://{}/from-a.md", f.b.site);
    let (out, error) = tool(
        &f.a,
        "write",
        json!({ "url": url, "content": "# from a\n" }),
    )
    .await;
    assert!(error, "{out}");
    assert!(
        !f.b.server
            .web
            .root()
            .join(&f.b.site)
            .join("from-a.md")
            .exists()
    );
}

// ---- Events: watches and reminders ------------------------------------------------------------

#[tokio::test]
async fn page_changes_and_reminders_stay_in_their_workspace() {
    let f = fixture().await;
    let mut a_events = f.a.server.hub.subscribe();
    // A change in B's web is not an event of A's (watches and clients hear A's hub only).
    let url = SiteUrl::new("memory", "watched.md").unwrap();
    f.b.server
        .write_page(&url, "# b's change\n", ChangedBy::Agent)
        .unwrap();
    let heard = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if let Ok(event) = a_events.recv().await
                && matches!(&*event, ServerMessage::PageChanged { url, .. } if url == "site://memory/watched.md")
            {
                return;
            }
        }
    })
    .await;
    assert!(heard.is_err(), "A heard B's page change");
    f.a.server
        .write_page(&url, "# a's change\n", ChangedBy::Agent)
        .unwrap();
    let heard = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(event) = a_events.recv().await
                && matches!(&*event, ServerMessage::PageChanged { url, .. } if url == "site://memory/watched.md")
            {
                return;
            }
        }
    })
    .await;
    assert!(heard.is_ok(), "A didn't hear its own page change");

    // Each reminder goes off in its own workspace, into one of its own sessions.
    let at = NaiveDateTime::parse_from_str("2099-01-01 09:00", "%Y-%m-%d %H:%M").unwrap();
    let a_sessions: Vec<String> =
        f.a.server
            .sessions
            .list()
            .into_iter()
            .map(|s| s.id)
            .collect();
    let fired = check_reminders(&f.b.server, at);
    assert_eq!(fired.len(), 1);
    assert!(
        f.a.server
            .sessions
            .list()
            .iter()
            .all(|s| a_sessions.contains(&s.id)),
        "B's reminder made a session in A"
    );
    let fired = check_reminders(&f.a.server, at);
    assert_eq!(fired.len(), 1, "A's own reminder fires once, B's never");
}

// ---- Links: mounts and symlinked sites --------------------------------------------------------

#[tokio::test]
async fn links_in_one_web_never_open_the_others_folders() {
    let f = fixture().await;
    let web = f.a.server.web.root().to_path_buf();
    // A link inside a site to A's own project (allowed), one to B's project, and a whole site
    // that is a symlink to B's site.
    std::fs::create_dir_all(web.join("mnt")).unwrap();
    std::fs::write(web.join("mnt/index.md"), "# mounts\n").unwrap();
    std::os::unix::fs::symlink(&f.a.project, web.join("mnt/own")).unwrap();
    std::os::unix::fs::symlink(&f.b.project, web.join("mnt/theirs")).unwrap();
    let b_site = f.b.server.web.root().join(&f.b.site);
    std::os::unix::fs::symlink(&b_site, web.join("b-site")).unwrap();
    f.a.server.reindex();

    let token = &f.a.server.token;
    let get = |path: &'static str| http(f.addr, "GET", path, token, None);
    assert_eq!(get("/w/default/api/files/mnt/own/README.md").await.0, 200);
    assert_ne!(
        get("/w/default/api/files/mnt/theirs/README.md").await.0,
        200
    );
    assert_ne!(get("/w/default/api/files/b-site/data.json").await.0, 200);
    assert_ne!(get("/w/default/b-site/").await.0, 200);
    let (out, _) = tool(&f.a, "open", json!({ "url": "site://b-site/index.md" })).await;
    assert!(!out.contains(&f.b.word), "{out}");
    let (out, _) = tool(&f.a, "search", json!({ "query": f.b.word })).await;
    assert!(!out.contains("b-site"), "{out}");
    assert!(f.a.server.web.allows(&web.join("mnt/own/README.md")));
}

// ---- Defense in depth: stale sessions and the launcher -----------------------------------------

#[tokio::test]
async fn the_launcher_opens_only_its_own_workspaces_projects() {
    let f = fixture().await;
    let token = &f.a.server.token;
    let (status, text) = http(f.addr, "POST", "/w/default/api/launcher/own", token, None).await;
    assert_eq!(status, 200, "{text}");
    let (status, text) = http(
        f.addr,
        "POST",
        "/w/default/api/launcher/theirs",
        token,
        None,
    )
    .await;
    assert_eq!(status, 403, "{text}");
}

#[tokio::test]
async fn a_session_left_in_the_other_workspaces_project_opens_none_of_it() {
    let f = fixture().await;
    // As a session from before ownership existed would be: in a project B owns.
    let stale =
        f.a.server
            .sessions
            .create("all", Some(f.b.project.clone()))
            .unwrap();
    let context = ToolContext {
        server: f.a.server.clone(),
        session: stale,
        agent: Arc::new(AgentDefinition::load(&f.a.server.web, "all").unwrap()),
        cancel: CancellationToken::new(),
    };
    let result =
        f.a.server
            .tools
            .call_value(
                &context,
                "open",
                json!({ "url": format!("{}/pic.png", f.b.project) }),
            )
            .await;
    assert!(
        result.is_error && result.images.is_empty(),
        "{}",
        result.text
    );
}

#[tokio::test]
async fn a_worker_session_id_cannot_climb_into_the_other_workspace() {
    let f = fixture().await;
    let climbing = format!("../../../b/state/sessions/{}-x", f.b.session);
    let info = SessionInfo {
        id: climbing,
        agent: "worker".into(),
        project: None,
        ticket: None,
        parent: None,
        worker: None,
        state: TurnState::Idle,
        last_seq: 0,
        context_tokens: 0,
        model: None,
        preview: None,
        updated: chrono::Utc::now().to_rfc3339(),
    };
    assert!(f.a.server.sessions.create_with_info(info).is_err());
    let landed =
        f.b.server
            .home
            .sessions()
            .join(format!("{}-x", f.b.session));
    assert!(!landed.exists(), "A made a session folder in B");
}

#[tokio::test]
async fn a_stale_session_shows_no_diff_of_the_other_workspaces_project() {
    let f = fixture().await;
    let stale =
        f.a.server
            .sessions
            .create("all", Some(f.b.project.clone()))
            .unwrap();
    let path = format!("/w/default/api/sessions/{}/project/diff", stale.id());
    let (status, text) = http(f.addr, "GET", &path, &f.a.server.token, None).await;
    assert_eq!(status, 403, "{text}");
    // A's own session still gets its diff.
    let path = format!("/w/default/api/sessions/{}/project/diff", f.a.session);
    assert_eq!(
        http(f.addr, "GET", &path, &f.a.server.token, None).await.0,
        200
    );
}
