//! A worker process uses an MCP server's tool: the parent lists the tool for it and runs the
//! call, so the approval reaches the parent's clients (and there is one MCP server process).

mod support;

use std::collections::VecDeque;

use composer_protocol::{ApprovalDecision, ClientCommand, CommandResult, ServerMessage};
use composer_server::{Home, Server, init_home};
use serde_json::json;
use support::{Reply, Scripts, fake_model_logged, send, until};

/// One tool, `add.numbers`.
const FAKE_SERVER: &str = r#"
import json, sys
for line in sys.stdin:
    m = json.loads(line)
    mid, method, p = m.get("id"), m.get("method"), m.get("params") or {}
    if mid is None:
        continue
    if method == "initialize":
        r = {"protocolVersion": p["protocolVersion"], "capabilities": {"tools": {}}, "serverInfo": {"name": "fake", "version": "1"}}
    elif method == "tools/list":
        r = {"tools": [{"name": "add.numbers", "description": "Adds a and b", "inputSchema": {"type": "object"}}]}
    else:
        a = p["arguments"]
        r = {"content": [{"type": "text", "text": "sum: %d" % (a["a"] + a["b"])}]}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": mid, "result": r}) + "\n")
    sys.stdout.flush()
"#;

#[tokio::test]
async fn a_worker_process_calls_an_mcp_tool_through_the_parent() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let (base_url, requests) = fake_model_logged(Scripts {
        orchestrator: VecDeque::from([
            Reply::Call(
                "spawn_worker",
                json!({ "ticket": "0001", "agent": "mcpworker" }),
            ),
            Reply::Text("worker started"),
            Reply::Text("ticket 0001 is done"),
        ]),
        worker: VecDeque::from([
            Reply::Call("fake__add_numbers", json!({ "a": 2, "b": 3 })),
            Reply::Call("finish", json!({ "status": "done", "summary": "added" })),
            Reply::Text("finished"),
        ]),
        ..Scripts::default()
    })
    .await;

    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake_mcp.py");
    std::fs::write(&script, FAKE_SERVER).unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let worker = std::fs::read_to_string(dir.path().join("sites/agents/worker.md")).unwrap();
    std::fs::write(
        dir.path().join("sites/agents/mcpworker.md"),
        worker
            .replace("name: worker", "name: mcpworker")
            .replace(
                "description: general worker; carries out any ticket",
                "description: adds numbers",
            )
            .replace("run, finish]", "run, finish, fake__*]"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("sites/orchestration/tickets")).unwrap();
    std::fs::write(
        dir.path().join("sites/orchestration/tickets/0001.md"),
        "---\ntitle: Add\ntype: ticket\nid: \"0001\"\nstatus: open\nagent: mcpworker\n---\n# Add\n\n## Task\nAdd 2 and 3.\n\n## Result\n",
    )
    .unwrap();
    std::fs::write(
        home.config_file(),
        format!(
            "[provider]\nbase_url = \"{base_url}\"\n\n[mcp.fake]\ncommand = \"python3\"\nargs = [\"{}\"]\n",
            script.display()
        ),
    )
    .unwrap();
    // SAFETY: set before any server or thread reads it; this test is the only one in this binary.
    unsafe { std::env::set_var("COMPOSER_WORKER_EXE", env!("CARGO_BIN_EXE_composer")) };
    let server = Server::start(home).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = server.clone();
    tokio::spawn(async move { serving.serve(listener).await });

    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws?token={}", server.token))
            .await
            .unwrap();
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "orchestrator".into(),
            project: None,
        },
    )
    .await;
    let reply = until(&mut socket, 10, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!()
    };
    let orchestrator = session.id.clone();
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: orchestrator.clone(),
            since: None,
        },
    )
    .await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: orchestrator.clone(),
            text: "add them".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;

    // The worker's call is asked here, in the worker's session.
    let events = until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest {
        request,
        session,
        title,
        ..
    }) = events.last()
    else {
        panic!()
    };
    assert_eq!(title, "MCP fake: add.numbers");
    assert_ne!(*session, orchestrator);
    let worker = session.clone();
    send(
        &mut socket,
        4,
        ClientCommand::SessionAttach {
            session: worker.clone(),
            since: None,
        },
    )
    .await;
    send(
        &mut socket,
        5,
        ClientCommand::ApprovalRespond {
            request: request.clone(),
            decision: ApprovalDecision::Approve,
        },
    )
    .await;
    let events = until(&mut socket, 60, |e| {
        matches!(e, ServerMessage::MessageDone { session, message, .. }
            if *session == orchestrator && message.content == "ticket 0001 is done")
    })
    .await;
    assert!(events.iter().any(|e| matches!(e, ServerMessage::ToolResult { session, result, is_error: false, .. }
        if *session == worker && result == "sum: 5")));
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(ticket.contains("status: done"), "{ticket}");

    // The worker's model was offered the tool (listed by the parent); the orchestrator's wasn't.
    let offered = |role_tool: &str| -> Vec<String> {
        requests
            .lock()
            .unwrap()
            .iter()
            .find(|r| {
                r["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["function"]["name"] == role_tool)
            })
            .unwrap()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(offered("finish").contains(&"fake__add_numbers".to_string()));
    assert!(!offered("spawn_worker").iter().any(|t| t.contains("__")));
}
