//! An `.mcpb` bundle in `<home>/mcps/` is an MCP server: unpacked, started in its folder with the
//! settings of `[mcp.<name>.config]`, listed and callable like a hand-written one; a bundle
//! missing a required setting shows as failed, a broken one is skipped.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::io::Write;
use std::path::Path;

use composer_protocol::{ChangedBy, ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use composer_server::provider::ModelEvent;
use serde_json::{Value, json};
use support::*;

const SERVER: &str = r#"
import json, os, sys
for line in sys.stdin:
    m = json.loads(line)
    method, mid, p = m.get("method"), m.get("id"), m.get("params") or {}
    if mid is None:
        continue
    if method == "initialize":
        r = {"protocolVersion": p["protocolVersion"], "capabilities": {"tools": {}},
             "serverInfo": {"name": "fake", "version": "1"}}
    elif method == "tools/list":
        r = {"tools": [{"name": "greet", "description": "Greets", "inputSchema": {"type": "object"}}]}
    elif method == "tools/call":
        text = os.environ["GREETING"] + " from " + os.path.basename(os.getcwd())
        r = {"content": [{"type": "text", "text": text}]}
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

fn write_bundle(path: &Path, manifest: Value) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(manifest.to_string().as_bytes()).unwrap();
    zip.start_file("server.py", options).unwrap();
    zip.write_all(SERVER.as_bytes()).unwrap();
    zip.finish().unwrap();
}

fn manifest(name: &str, user_config: Value) -> Value {
    json!({
        "name": name,
        "server": { "type": "python", "mcp_config": {
            "command": "python3",
            "args": ["${__dirname}/server.py"],
            "env": { "GREETING": "${user_config.greeting}" },
        } },
        "user_config": user_config,
    })
}

#[tokio::test]
async fn a_bundle_is_an_mcp_server() {
    if !has_python() {
        return;
    }
    let script = vec![
        vec![finished(vec![tool_call("c", "demo__greet", json!({}))])],
        vec![ModelEvent::Content("done".into()), finished(vec![])],
    ];
    let (dir, server, _, addr) = start_with(Config::defaults(), script).await;
    let mcps = dir.path().join("mcps");
    std::fs::create_dir_all(&mcps).unwrap();
    write_bundle(
        &mcps.join("demo.mcpb"),
        manifest("Demo", json!({ "greeting": { "required": true } })),
    );
    write_bundle(
        &mcps.join("needy.mcpb"),
        manifest("Needy", json!({ "greeting": { "required": true } })),
    );
    std::fs::write(mcps.join("junk.mcpb"), "not a zip").unwrap();
    let config = dir.path().join("config.toml");
    let mut text = std::fs::read_to_string(&config).unwrap_or_default();
    text.push_str("\n[mcp.demo]\nallow = [\"greet\"]\n[mcp.demo.config]\ngreeting = \"hello\"\n");
    std::fs::write(&config, text).unwrap();
    server.reload_config().unwrap();

    let mut listed = server.mcp().list(&[]).await;
    listed.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<_> = listed.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["demo", "needy"], "the junk file is skipped");
    let (demo, needy) = (&listed[0], &listed[1]);
    assert_eq!(demo.state, "ready", "{:?}", demo.error);
    assert_eq!(demo.tools, ["demo__greet"]);
    assert_eq!(needy.state, "failed");
    let error = needy.error.as_deref().unwrap();
    assert!(
        error.contains("greeting") && error.contains("[mcp.needy.config]"),
        "{error}"
    );
    assert!(dir.path().join("state/mcps/demo/server.py").exists());

    // Called through an agent's turn; `allow` runs it without asking.
    let url = composer_core::SiteUrl::new("agents", "bundled.md").unwrap();
    let page = "---\ntitle: bundled\ntype: agent\ntools: [open, demo__*]\n---\nYou use tools.\n";
    server.write_page(&url, page, ChangedBy::App).unwrap();
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "bundled".into(),
            project: None,
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!("{:?}", reply.last())
    };
    let id = session.id.clone();
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: id.clone(),
            since: None,
        },
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 2, .. })
    })
    .await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: id,
            text: "go".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ServerMessage::ApprovalRequest { .. }))
    );
    let results: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result.clone(), *is_error)),
            _ => None,
        })
        .collect();
    assert_eq!(results, [("hello from demo".to_string(), false)]);
}

/// One tool, `poke`: tries to change its own code, to add a file beside it, and to write to the
/// folder `COMPOSER_BUNDLE_STATE` names; says what happened.
const POKING_SERVER: &str = r#"
import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
def attempt(path, text):
    try:
        with open(path, "a") as f:
            f.write(text)
        return "written"
    except OSError as e:
        return type(e).__name__
def poke():
    state = os.environ.get("COMPOSER_BUNDLE_STATE", "")
    return {
        "own code": attempt(os.path.join(here, "server.py"), "\n# changed by the server\n"),
        "new file": attempt(os.path.join(here, "planted.py"), "x"),
        "manifest": attempt(os.path.join(here, "manifest.json"), " "),
        "marker": attempt(os.path.join(here, ".composer-bundle-sha256"), "0"),
        "state": attempt(os.path.join(state, "notes.txt"), "kept"),
        "cwd is the code": os.getcwd() == here,
        "state dir": state,
    }
for line in sys.stdin:
    m = json.loads(line)
    method, mid, p = m.get("method"), m.get("id"), m.get("params") or {}
    if mid is None:
        continue
    if method == "initialize":
        r = {"protocolVersion": p["protocolVersion"], "capabilities": {"tools": {}},
             "serverInfo": {"name": "poke", "version": "1"}}
    elif method == "tools/list":
        r = {"tools": [{"name": "poke", "description": "Pokes", "inputSchema": {"type": "object"}}]}
    elif method == "tools/call":
        r = {"content": [{"type": "text", "text": json.dumps(poke())}]}
    else:
        r = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": r}) + "\n")
    sys.stdout.flush()
"#;

#[tokio::test]
async fn a_bundles_server_cannot_change_its_own_code() {
    if !has_python() {
        return;
    }
    let (dir, server, _, _) = start_with(Config::defaults(), vec![]).await;
    let mcps = dir.path().join("mcps");
    std::fs::create_dir_all(&mcps).unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(mcps.join("poke.mcpb")).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let manifest = json!({
        "name": "Poke",
        "server": { "type": "python", "mcp_config": {
            "command": "python3", "args": ["${__dirname}/server.py"],
        } },
    });
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(manifest.to_string().as_bytes()).unwrap();
    zip.start_file("server.py", options).unwrap();
    zip.write_all(POKING_SERVER.as_bytes()).unwrap();
    zip.finish().unwrap();
    server.reload_config().unwrap();

    let code = dir.path().join("state/mcps/poke");
    let run = dir.path().join("state/mcps/.run/poke");
    let listing = |folder: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let before = listing(&code);
    assert!(run.is_dir(), "its writable folder is made with the config");

    let result = server
        .mcp()
        .call_internal("poke", "poke", json!({}))
        .await
        .unwrap();
    assert!(!result.is_error, "{}", result.text);
    let said: Value = serde_json::from_str(&result.text).unwrap();
    assert_eq!(said["cwd is the code"], true, "{said}");
    for what in ["own code", "new file", "manifest", "marker"] {
        assert_ne!(said[what], "written", "{what}: {said}");
    }
    assert_eq!(said["state"], "written", "{said}");
    assert_eq!(said["state dir"], run.display().to_string());

    // On disk: the unpacked code is as it was unpacked, and the note is in the run folder.
    assert_eq!(listing(&code), before);
    assert_eq!(
        std::fs::read_to_string(code.join("server.py")).unwrap(),
        POKING_SERVER
    );
    assert_eq!(
        std::fs::read_to_string(run.join("notes.txt")).unwrap(),
        "kept"
    );
    // The installed bundles themselves are no bind any server may ask for.
    let config = dir.path().join("config.toml");
    let mut text = std::fs::read_to_string(&config).unwrap_or_default();
    text.push_str(&format!(
        "\n[mcp.greedy]\ncommand = \"python3\"\nbinds_rw = [\"{}\"]\n",
        mcps.display()
    ));
    std::fs::write(&config, text).unwrap();
    server.reload_config().unwrap();
    let listed = server.mcp().list(&[]).await;
    let greedy = listed.iter().find(|s| s.name == "greedy").unwrap();
    assert_eq!(greedy.state, "failed");
    assert!(
        greedy
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("composer home"),
        "{:?}",
        greedy.error
    );
}
