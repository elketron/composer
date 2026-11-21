//! A site's own MCP servers, declared in its `mcp.toml`: nothing starts until the user approved
//! the file's exact bytes (an edit asks again), setting the site starts them (pooled) and offers
//! their tools (all to agentic runs, the `read_only` ones to workflow runs), and composer's MCP
//! server tells its client the list changed. The server is a fake stdio one in Python that notes
//! each start.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use composer_protocol::{ApprovalDecision, ClientCommand, RunKind, ServerMessage};
use composer_server::agent::{Entered, Site, SiteSet, set_site};
use composer_server::mcp::{McpServer, ServerLink};
use composer_server::tools::ToolContext;
use composer_server::{Config, Server};
use serde_json::{Value, json};
use support::*;
use tokio_util::sync::CancellationToken;

/// Tools `echo` and `launch`. Each start appends a line to `started.txt` next to the script.
const FAKE_SERVER: &str = r#"
import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(here, "started.txt"), "a") as f:
    f.write("started\n")
tools = [
    {"name": "echo", "description": "Echoes text", "inputSchema": {"type": "object"}},
    {"name": "launch", "description": "Starts an app", "inputSchema": {"type": "object"}},
]
for line in sys.stdin:
    m = json.loads(line)
    method, mid, p = m.get("method"), m.get("id"), m.get("params") or {}
    if mid is None:
        continue
    if method == "initialize":
        r = {"protocolVersion": p["protocolVersion"], "capabilities": {"tools": {}},
             "serverInfo": {"name": "fake", "version": "1"}}
    elif method == "tools/list":
        r = {"tools": tools}
    elif method == "tools/call":
        args = p.get("arguments") or {}
        r = {"content": [{"type": "text", "text": args.get("text", "launched")}]}
    else:
        r = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": r}) + "\n")
    sys.stdout.flush()
"#;

fn has_python() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The script in `scratch`, and an `mcp.toml` entry `[servers.<name>]` that runs it.
fn fake_entry(scratch: &std::path::Path, name: &str, extra: &str) -> String {
    let script = scratch.join("fake_mcp.py");
    std::fs::write(&script, FAKE_SERVER).unwrap();
    format!(
        "[servers.{name}]\ncommand = \"python3\"\nargs = [\"{}\"]\n{extra}",
        script.display()
    )
}

fn starts(scratch: &std::path::Path) -> usize {
    std::fs::read_to_string(scratch.join("started.txt"))
        .unwrap_or_default()
        .lines()
        .count()
}

/// Answers every approval request with what `decision` holds at the time, and keeps the
/// bodies it was asked with.
struct Answerer {
    decision: Arc<Mutex<ApprovalDecision>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Answerer {
    fn start(server: &Arc<Server>, decision: ApprovalDecision) -> Self {
        let decision = Arc::new(Mutex::new(decision));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let mut events = server.hub.subscribe();
        let (server, answer, bodies) = (server.clone(), decision.clone(), asked.clone());
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if let ServerMessage::ApprovalRequest {
                    request,
                    recipe,
                    body,
                    ..
                } = &*event
                {
                    assert_eq!(recipe, "mcp.toml");
                    bodies.lock().unwrap().push(body.clone());
                    let decision = *answer.lock().unwrap();
                    server.pending_approvals.resolve(request, decision);
                }
            }
        });
        Self { decision, asked }
    }

    fn answer(&self, decision: ApprovalDecision) {
        *self.decision.lock().unwrap() = decision;
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

fn context(
    server: &Arc<Server>,
    session: &Arc<composer_server::session::Session>,
    kind: RunKind,
) -> ToolContext {
    let site = session
        .info()
        .site
        .as_deref()
        .and_then(|name| Site::load(&server.web, name).ok())
        .map(Arc::new);
    ToolContext {
        server: server.clone(),
        run: server.sink(session),
        kind,
        only: None,
        site,
        cancel: CancellationToken::new(),
    }
}

async fn enter(
    server: &Arc<Server>,
    session: &Arc<composer_server::session::Session>,
    name: &str,
) -> Entered {
    let cancel = CancellationToken::new();
    match set_site(server, &*server.sink(session), Some(name), &cancel)
        .await
        .unwrap()
    {
        SiteSet::Entered(entered) => entered,
        other => panic!("{other:?}"),
    }
}

async fn offered(server: &Arc<Server>, context: &ToolContext) -> Vec<String> {
    server
        .tools
        .specs_with_mcp(server, context.kind, context.site.as_deref())
        .await
        .into_iter()
        .map(|spec| spec.name)
        .collect()
}

fn site(dir: &std::path::Path, name: &str, mcp_toml: Option<&str>) {
    write(
        &dir.join(format!("sites/{name}/index.md")),
        &format!("---\ntitle: {name}\n---\n# {name}\n"),
    );
    if let Some(text) = mcp_toml {
        write(&dir.join(format!("sites/{name}/mcp.toml")), text);
    }
}

#[tokio::test]
async fn nothing_starts_until_the_user_approved_these_bytes() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let (dir, server, _provider, _addr) = start_with(Config::defaults(), vec![]).await;
    let declared = format!(
        "{}\n[servers.broken]\nbundle = \"b\"\nurl = \"http://x/y\"\n",
        fake_entry(scratch.path(), "fake", "allow = [\"echo\"]\n")
    );
    site(dir.path(), "lab", Some(&declared));
    let answers = Answerer::start(&server, ApprovalDecision::Deny);
    let session = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();

    // Refused: asked once, with the command and the file in the question; nothing started,
    // nothing offered, and the entry that did not read is said.
    let entered = enter(&server, &session, "lab").await;
    assert!(!entered.mcp.approved);
    let notes = entered.notes();
    assert!(notes.contains("not approved"), "{notes}");
    assert!(
        notes.contains("line 6: server `broken` names more than one server"),
        "{notes}"
    );
    assert_eq!(answers.asked().len(), 1);
    assert!(
        answers.asked()[0].contains("fake: runs `python3 "),
        "{:?}",
        answers.asked()
    );
    assert!(
        answers.asked()[0].contains(&declared),
        "the file itself is shown"
    );
    assert_eq!(starts(scratch.path()), 0, "nothing started");
    let inside = context(&server, &session, RunKind::Agentic);
    assert!(
        !offered(&server, &inside)
            .await
            .iter()
            .any(|n| n.starts_with("fake__"))
    );
    let refused = server
        .tools
        .call_value(&inside, "fake__echo", json!({ "text": "hi" }))
        .await;
    assert!(refused.text.contains("unknown tool"), "{}", refused.text);
    assert_eq!(starts(scratch.path()), 0);

    // Approved: started now, offered, and runs (`allow`).
    answers.answer(ApprovalDecision::Approve);
    let entered = enter(&server, &session, "lab").await;
    assert!(entered.mcp.approved, "{}", entered.notes());
    assert_eq!(answers.asked().len(), 2);
    assert_eq!(starts(scratch.path()), 1);
    let inside = context(&server, &session, RunKind::Agentic);
    let names = offered(&server, &inside).await;
    assert!(names.contains(&"fake__echo".to_string()), "{names:?}");
    let ran = server
        .tools
        .call_value(&inside, "fake__echo", json!({ "text": "hi" }))
        .await;
    assert_eq!((ran.text.as_str(), ran.is_error), ("hi", false));

    // Remembered, also past a restart (the recipe store on disk): not asked again.
    enter(&server, &session, "lab").await;
    assert_eq!(answers.asked().len(), 2);
    let stored = std::fs::read_to_string(server.home.approvals_file()).unwrap();
    assert!(stored.contains("\"mcp.toml\""), "{stored}");

    // Another site naming the same server: the same process, not a second one.
    site(
        dir.path(),
        "lab2",
        Some(&fake_entry(scratch.path(), "twin", "")),
    );
    answers.answer(ApprovalDecision::Always);
    let entered = enter(&server, &session, "lab2").await;
    assert!(entered.mcp.approved);
    let names = offered(&server, &context(&server, &session, RunKind::Agentic)).await;
    assert!(names.contains(&"twin__echo".to_string()), "{names:?}");
    assert!(
        !names.contains(&"fake__echo".to_string()),
        "lab's left with lab"
    );
    assert_eq!(starts(scratch.path()), 1, "pooled");

    // An edit is new bytes: refused, so the site's servers are gone, and asked again.
    answers.answer(ApprovalDecision::Deny);
    write(
        &dir.path().join("sites/lab/mcp.toml"),
        &format!("{declared}\n# edited\n"),
    );
    let entered = enter(&server, &session, "lab").await;
    assert!(!entered.mcp.approved);
    assert_eq!(answers.asked().len(), 4);
    let inside = context(&server, &session, RunKind::Agentic);
    assert!(
        !offered(&server, &inside)
            .await
            .iter()
            .any(|n| n.starts_with("fake__"))
    );

    // An edit nobody answered is not used either, however the run reaches the site.
    let other = server
        .sessions
        .create(RunKind::Agentic, None, Some("lab".into()))
        .unwrap();
    let names = offered(&server, &context(&server, &other, RunKind::Agentic)).await;
    assert!(!names.iter().any(|n| n.starts_with("fake__")), "{names:?}");
}

#[tokio::test]
async fn a_workflow_run_gets_only_the_read_only_tools() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let (dir, server, _provider, _addr) = start_with(Config::defaults(), vec![]).await;
    let declared = fake_entry(
        scratch.path(),
        "fake",
        "allow = [\"*\"]\nread_only = [\"echo\"]\n",
    );
    site(dir.path(), "lab", Some(&declared));
    site(
        dir.path(),
        "plain",
        Some(&fake_entry(scratch.path(), "other", "allow = [\"*\"]\n")),
    );
    Answerer::start(&server, ApprovalDecision::Approve);
    let session = server
        .sessions
        .create(RunKind::Workflow, None, None)
        .unwrap();
    enter(&server, &session, "lab").await;

    let agentic = offered(&server, &context(&server, &session, RunKind::Agentic)).await;
    assert!(agentic.contains(&"fake__echo".to_string()));
    assert!(agentic.contains(&"fake__launch".to_string()));
    let workflow = context(&server, &session, RunKind::Workflow);
    let names = offered(&server, &workflow).await;
    assert!(names.contains(&"fake__echo".to_string()), "{names:?}");
    assert!(!names.contains(&"fake__launch".to_string()), "{names:?}");
    let refused = server
        .tools
        .call_value(&workflow, "fake__launch", json!({}))
        .await;
    assert!(refused.is_error && refused.text.contains("unknown tool"));
    let ran = server
        .tools
        .call_value(&workflow, "fake__echo", json!({ "text": "x" }))
        .await;
    assert_eq!(ran.text, "x");
    let single = context(&server, &session, RunKind::SingleShot);
    assert!(offered(&server, &single).await.is_empty());

    // None by default.
    enter(&server, &session, "plain").await;
    let names = offered(&server, &context(&server, &session, RunKind::Workflow)).await;
    assert!(!names.iter().any(|n| n.starts_with("other__")), "{names:?}");
}

/// Reads lines from composer's MCP server until a list_changed notification.
async fn list_changed(lines: &mut tokio::sync::mpsc::UnboundedReceiver<String>) {
    loop {
        let line = tokio::time::timeout(Duration::from_secs(20), lines.recv())
            .await
            .expect("a notification")
            .expect("a line");
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["method"] == "notifications/tools/list_changed" {
            return;
        }
    }
}

async fn listed(server: &McpServer) -> Vec<String> {
    let line = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/list" }).to_string();
    let reply: Value = serde_json::from_str(&server.handle(&line).await.unwrap()).unwrap();
    reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn setting_the_site_changes_the_mcp_servers_list_and_says_so() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let (dir, server, _provider, addr) = start_with(Config::defaults(), vec![]).await;
    site(
        dir.path(),
        "lab",
        Some(&fake_entry(scratch.path(), "fake", "")),
    );
    Answerer::start(&server, ApprovalDecision::Approve);
    let session = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();

    // Composer's MCP server for the session, as `composer mcp` runs it.
    let link = ServerLink::open(&format!("http://{addr}"), &server.token, session.id())
        .await
        .unwrap();
    let (out, mut lines) = tokio::sync::mpsc::unbounded_channel();
    let mcp = Arc::new(McpServer::new(link, out));
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
    let reply: Value = serde_json::from_str(&mcp.handle(&init.to_string()).await.unwrap()).unwrap();
    assert_eq!(
        reply["result"]["capabilities"]["tools"]["listChanged"],
        true
    );
    let following = mcp.clone();
    tokio::spawn(async move { following.follow_changes().await });
    assert!(!listed(&mcp).await.contains(&"fake__echo".to_string()));

    // The user sets the site: the client hears the list changed, and the new list has the
    // site's server's tools.
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SiteSet {
            session: session.id().to_string(),
            site: Some("lab".into()),
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    list_changed(&mut lines).await;
    let names = listed(&mcp).await;
    assert!(names.contains(&"fake__echo".to_string()), "{names:?}");
    assert!(names.contains(&"open".to_string()), "{names:?}");

    // Leaving: said again, and they are gone.
    send(
        &mut socket,
        2,
        ClientCommand::SiteSet {
            session: session.id().to_string(),
            site: None,
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    list_changed(&mut lines).await;
    assert!(!listed(&mcp).await.contains(&"fake__echo".to_string()));
}
