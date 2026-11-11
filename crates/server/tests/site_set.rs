//! `site.set` over the socket: the user enters a workflow site or leaves it. The reply is a
//! `note` with what happened (the application's outcome included); the session's new state
//! arrives as `session.updated`.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up fakes outside the sandbox"
)]

mod support;

use std::collections::BTreeMap;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::{ApplicationConfig, Config, McpServerConfig};
use support::*;

/// An MCP server with `launch` and `list_apps` only.
const FAKE_SERVER: &str = r#"
import json, sys
tools = [
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
        r = {"content": [{"type": "text", "text": "ok" if p["name"] == "launch" else "no apps"}]}
    else:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "no " + method}})
        continue
    send({"jsonrpc": "2.0", "id": mid, "result": r})
"#;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn config(scratch: &std::path::Path) -> Config {
    let script = scratch.join("fake_mcp.py");
    std::fs::write(&script, FAKE_SERVER).unwrap();
    let mut config = Config::defaults();
    config.mcp.insert(
        "fake".into(),
        McpServerConfig {
            command: "python3".into(),
            args: vec![script.display().to_string()],
            binds_rw: vec![scratch.display().to_string()],
            enabled: true,
            ..Default::default()
        },
    );
    // `blender` is configured; `ghost` (the `lost` site's) is not.
    config.applications.insert(
        "blender".into(),
        ApplicationConfig {
            command: "blender".into(),
            args: vec![],
            env: BTreeMap::new(),
            mcp: "fake".into(),
        },
    );
    config
}

/// Sends `site.set` and returns the reply (`Ok` result or the error message) with the events
/// that came before it.
async fn site_set(
    socket: &mut Socket,
    id: u64,
    session: &str,
    site: Option<&str>,
) -> (Result<CommandResult, String>, Vec<ServerMessage>) {
    send(
        socket,
        id,
        ClientCommand::SiteSet {
            session: session.into(),
            site: site.map(Into::into),
        },
    )
    .await;
    let mut events = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    let reply = match events.pop().unwrap() {
        ServerMessage::Ok { result, .. } => Ok(result),
        ServerMessage::Error { message, .. } => Err(message),
        other => panic!("{other:?}"),
    };
    (reply, events)
}

fn note(reply: Result<CommandResult, String>) -> String {
    match reply {
        Ok(CommandResult::Note { text }) => text,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_user_enters_and_leaves_a_site() {
    let has_python = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let scratch = tempfile::tempdir().unwrap();
    let (dir, server, _provider, addr) = start_with(config(scratch.path()), vec![]).await;
    let sites = dir.path().join("sites");
    write(
        &sites.join("plain/index.md"),
        "---\ntitle: Plain\n---\n# Plain\n",
    );
    write(
        &sites.join("3d/index.md"),
        "---\ntitle: 3D\napp: blender\n---\n# 3D\n",
    );
    write(
        &sites.join("lost/index.md"),
        "---\ntitle: Lost\napp: ghost\n---\n# Lost\n",
    );
    write(
        &sites.join("game/index.md"),
        "---\ntitle: Game\nproject: required\n---\n# Game\n",
    );
    let session = server.sessions.create("default", None).unwrap();
    let id = session.id().to_string();
    let mut socket = connect(addr, &server.token).await;

    // A site without an application: entered, and the session update goes out.
    let (reply, events) = site_set(&mut socket, 1, &id, Some("plain")).await;
    let text = note(reply);
    assert!(text.contains("Entered plain"), "{text}");
    assert!(!text.contains("application"), "{text}");
    assert_eq!(session.info().site.as_deref(), Some("plain"));
    // The session update is the existing broadcast, not a second path (it may overtake the reply).
    let seen = |e: &ServerMessage| matches!(e, ServerMessage::SessionUpdated { session: s } if s.site.as_deref() == Some("plain"));
    if !events.iter().any(seen) {
        until(&mut socket, seen).await;
    }

    // `None` leaves; again, nothing to leave.
    let (reply, _) = site_set(&mut socket, 2, &id, None).await;
    assert!(note(reply).contains("Left site plain"));
    assert_eq!(session.info().site, None);
    let (reply, _) = site_set(&mut socket, 3, &id, None).await;
    assert!(note(reply).contains("not in a site"));

    // An application nobody configured: entered, with the reason.
    let (reply, _) = site_set(&mut socket, 4, &id, Some("lost")).await;
    let text = note(reply);
    assert!(
        text.contains("Entered lost") && text.contains("ghost application was not started"),
        "{text}"
    );
    assert_eq!(session.info().site.as_deref(), Some("lost"));

    // An application the fake server launches.
    if has_python {
        let (reply, _) = site_set(&mut socket, 5, &id, Some("3d")).await;
        let text = note(reply);
        assert!(text.contains("Started the blender application."), "{text}");
        assert_eq!(session.info().site.as_deref(), Some("3d"));
    }

    // Errors change nothing: an unknown site, a site that needs a project, an unknown session.
    let before = session.info().site;
    let (reply, _) = site_set(&mut socket, 6, &id, Some("nope")).await;
    let message = reply.unwrap_err();
    assert!(message.contains("nope"), "{message}");
    let (reply, _) = site_set(&mut socket, 7, &id, Some("game")).await;
    let message = reply.unwrap_err();
    assert!(message.contains("needs an active project"), "{message}");
    assert_eq!(session.info().site, before);
    let (reply, _) = site_set(&mut socket, 8, "no-such-session", Some("plain")).await;
    assert!(reply.is_err());

    // With a project the gated site opens.
    session.update_info(&server.hub, |info| {
        info.project = Some(dir.path().display().to_string())
    });
    let (reply, _) = site_set(&mut socket, 9, &id, Some("game")).await;
    assert!(note(reply).contains("Entered game"));
    assert_eq!(session.info().site.as_deref(), Some("game"));
}
