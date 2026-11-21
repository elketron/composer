//! A site's `mcp:` grants an MCP server's tools while a session is in it, where the server's
//! `[mcp.<name>] sites` lists the site (the page alone grants nothing). The server is a fake
//! stdio one in Python that records the calls it gets.

mod support;

use std::sync::Arc;

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_server::agent::{AgentDefinition, Site, enter_site_as_client, leave_site};
use composer_server::tools::ToolContext;
use composer_server::{Config, McpServerConfig, Server};
use serde_json::{Value, json};
use support::*;
use tokio_util::sync::CancellationToken;

/// Tools `echo`, `launch` and `list_apps`. Every call is appended to `calls.jsonl` next to the
/// script; `list_apps` answers with the text of `running.txt` there, when it exists.
const FAKE_SERVER: &str = r#"
import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
tools = [
    {"name": "echo", "description": "Echoes text", "inputSchema": {"type": "object"}},
    {"name": "launch", "description": "Starts an app", "inputSchema": {"type": "object"}},
    {"name": "list_apps", "description": "Lists apps", "inputSchema": {"type": "object"}},
]
def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()
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
        name, args = p["name"], p.get("arguments") or {}
        with open(os.path.join(here, "calls.jsonl"), "a") as log:
            log.write(json.dumps({"name": name, "arguments": args}) + "\n")
        text = args.get("text", "") if name == "echo" else "launched"
        if name == "list_apps":
            path = os.path.join(here, "running.txt")
            text = open(path).read() if os.path.exists(path) else "no apps"
        r = {"content": [{"type": "text", "text": text}]}
    else:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "no " + method}})
        continue
    send({"jsonrpc": "2.0", "id": mid, "result": r})
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

/// The fake server as `[mcp.fake]`, its files in the returned scratch directory. The sites
/// `lab` and `mcponly` may grant it.
fn fake_config(scratch: &std::path::Path) -> Config {
    let script = scratch.join("fake_mcp.py");
    std::fs::write(&script, FAKE_SERVER).unwrap();
    let mut config = Config::defaults();
    config.mcp.insert(
        "fake".into(),
        McpServerConfig {
            command: "python3".into(),
            args: vec![script.display().to_string()],
            allow: vec!["echo".into()],
            sites: vec!["lab".into(), "mcponly".into()],
            enabled: true,
            ..Default::default()
        },
    );
    config
}

fn calls(scratch: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(scratch.join("calls.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn context(
    server: &Arc<Server>,
    session: &Arc<composer_server::session::Session>,
    agent: &str,
) -> ToolContext {
    let site = session
        .info()
        .site
        .as_deref()
        .and_then(|name| Site::load(&server.web, name).ok())
        .map(Arc::new);
    ToolContext {
        server: server.clone(),
        session: session.clone(),
        agent: Arc::new(AgentDefinition::load(&server.web, agent).unwrap()),
        site,
        cancel: CancellationToken::new(),
    }
}

async fn offered(server: &Arc<Server>, context: &ToolContext) -> Vec<String> {
    server
        .tools
        .specs_with_mcp(server, &context.agent, context.site.as_deref())
        .await
        .into_iter()
        .map(|spec| spec.name)
        .collect()
}

fn write_agent(server: &Server) {
    server
        .write_page(
            &SiteUrl::new("agents", "base.md").unwrap(),
            "---\ntitle: base\ntype: agent\nname: base\ntools: [open, enter_site, leave_site]\nsites: all\n---\nYou are base.\n",
            ChangedBy::External,
        )
        .unwrap();
}

#[tokio::test]
async fn a_site_grants_mcp_tools_while_the_session_is_in_it() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let (dir, server, _provider, _addr) = start_with(fake_config(scratch.path()), vec![]).await;
    write_agent(&server);
    write(
        &dir.path().join("sites/lab/index.md"),
        "---\ntitle: Lab\nmcp: [fake, not_configured]\n---\n# Lab\n",
    );
    write(
        &dir.path().join("sites/plain/index.md"),
        "---\ntitle: Plain\n---\n# Plain\n",
    );
    let session = server.sessions.create("base", None).unwrap();

    // Outside: not offered, and a call is unknown.
    let outside = context(&server, &session, "base");
    assert!(
        !offered(&server, &outside)
            .await
            .contains(&"fake__echo".to_string())
    );
    let refused = server
        .tools
        .call_value(&outside, "fake__echo", json!({ "text": "hi" }))
        .await;
    assert!(
        refused.is_error && refused.text.contains("unknown tool"),
        "{}",
        refused.text
    );

    // Inside: offered (a server in `mcp:` that is not configured is skipped, and said so), and
    // the call runs without asking (`allow`).
    let entered = enter_site_as_client(&server, &session, "lab")
        .await
        .unwrap();
    assert_eq!(entered.mcp_refused.len(), 1, "{:?}", entered.mcp_refused);
    assert!(
        entered.mcp_refused[0].contains("there is no [mcp.not_configured]"),
        "{:?}",
        entered.mcp_refused
    );
    let inside = context(&server, &session, "base");
    let names = offered(&server, &inside).await;
    assert!(names.contains(&"fake__echo".to_string()), "{names:?}");
    assert!(names.contains(&"open".to_string()));
    let info = server
        .tools
        .infos_for(&server.mcp(), &inside.agent, inside.site.as_deref())
        .await;
    assert!(info.iter().any(|t| t.name == "fake__echo"));
    let ran = server
        .tools
        .call_value(&inside, "fake__echo", json!({ "text": "hi" }))
        .await;
    assert!(!ran.is_error && ran.text == "hi", "{}", ran.text);

    // Another site without `mcp:` takes it away again.
    enter_site_as_client(&server, &session, "plain")
        .await
        .unwrap();
    let other = context(&server, &session, "base");
    assert!(
        !offered(&server, &other)
            .await
            .contains(&"fake__echo".to_string())
    );
    assert!(
        server
            .tools
            .call_value(&other, "fake__echo", json!({}))
            .await
            .is_error
    );

    // Leaving too.
    enter_site_as_client(&server, &session, "lab")
        .await
        .unwrap();
    assert_eq!(leave_site(&server, &session).as_deref(), Some("lab"));
    let after = context(&server, &session, "base");
    let refused = server
        .tools
        .call_value(&after, "fake__echo", json!({ "text": "hi" }))
        .await;
    assert!(
        refused.is_error && refused.text.contains("unknown tool"),
        "{}",
        refused.text
    );
}

#[tokio::test]
async fn a_page_cannot_grant_a_server_its_config_does_not_list_the_site_for() {
    if !has_python() {
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let (dir, server, _provider, addr) = start_with(fake_config(scratch.path()), vec![]).await;
    write_agent(&server);
    // Any agent can write this page; `[mcp.fake] sites` lists `lab` and `mcponly` only.
    write(
        &dir.path().join("sites/sneaky/index.md"),
        "---\ntitle: Sneaky\nmcp: [fake]\n---\n# Sneaky\n",
    );
    write(
        &dir.path().join("sites/lab/index.md"),
        "---\ntitle: Lab\nmcp: fake\n---\n# Lab\n",
    );
    let session = server.sessions.create("base", None).unwrap();
    let says = "site sneaky names MCP server fake, but [mcp.fake] does not list it in `sites`";

    // The user enters: told in words, and nothing is granted.
    let entered = enter_site_as_client(&server, &session, "sneaky")
        .await
        .unwrap();
    assert_eq!(entered.mcp_refused.len(), 1);
    assert!(
        entered.mcp_refused[0].contains(says),
        "{:?}",
        entered.mcp_refused
    );
    assert!(entered.notes().contains(says), "{}", entered.notes());
    let inside = context(&server, &session, "base");
    assert_eq!(session.info().site.as_deref(), Some("sneaky"));
    let names = offered(&server, &inside).await;
    assert!(!names.iter().any(|n| n.starts_with("fake__")), "{names:?}");
    let info = server
        .tools
        .infos_for(&server.mcp(), &inside.agent, inside.site.as_deref())
        .await;
    assert!(!info.iter().any(|t| t.name.starts_with("fake__")));
    let refused = server
        .tools
        .call_value(&inside, "fake__echo", json!({ "text": "hi" }))
        .await;
    assert!(
        refused.is_error && refused.text.contains("unknown tool"),
        "{}",
        refused.text
    );
    assert!(
        calls(scratch.path()).is_empty(),
        "the server was never called"
    );

    // The agent enters: its tool result says the same.
    leave_site(&server, &session);
    let result = server
        .tools
        .call_value(
            &context(&server, &session, "base"),
            "enter_site",
            json!({ "site": "sneaky" }),
        )
        .await;
    assert!(!result.is_error, "{}", result.text);
    assert!(result.text.contains(says), "{}", result.text);

    // The user's command (`site.set`) says it in its note.
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        composer_protocol::ClientCommand::SiteSet {
            session: session.id().to_string(),
            site: Some("sneaky".into()),
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, composer_protocol::ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(composer_protocol::ServerMessage::Ok {
        result: composer_protocol::CommandResult::Note { text },
        ..
    }) = reply.last()
    else {
        panic!("{:?}", reply.last())
    };
    assert!(text.contains(says), "{text}");

    // A listed site grants it, and says nothing.
    let entered = enter_site_as_client(&server, &session, "lab")
        .await
        .unwrap();
    assert!(entered.mcp_refused.is_empty(), "{:?}", entered.mcp_refused);
    let inside = context(&server, &session, "base");
    assert!(
        offered(&server, &inside)
            .await
            .contains(&"fake__echo".to_string())
    );

    // The config decides, live: without `sites`, the listed site grants nothing either.
    let mut config = fake_config(scratch.path());
    config.mcp.get_mut("fake").unwrap().sites.clear();
    let (dir, server, _provider, _addr) = start_with(config, vec![]).await;
    write_agent(&server);
    write(
        &dir.path().join("sites/lab/index.md"),
        "---\ntitle: Lab\nmcp: fake\n---\n# Lab\n",
    );
    let session = server.sessions.create("base", None).unwrap();
    let entered = enter_site_as_client(&server, &session, "lab")
        .await
        .unwrap();
    assert_eq!(entered.mcp_refused.len(), 1);
    let inside = context(&server, &session, "base");
    assert!(
        !offered(&server, &inside)
            .await
            .contains(&"fake__echo".to_string())
    );
    assert!(
        server
            .tools
            .call_value(&inside, "fake__echo", json!({}))
            .await
            .is_error
    );
}
