//! `composer mcp` serves a running worker's tools: an external harness reads its ticket and
//! finishes it through MCP, and the orchestrator hears about it as from any worker. The
//! orchestrator works in a git project, so the worker gets its own worktree.

mod support;

use std::process::Stdio;
use std::time::Duration;

use composer_protocol::{ServerMessage, WorkerState};
use serde_json::{Value, json};
use support::{BusyWorker, busy_worker, until};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout};

struct Mcp {
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl Mcp {
    async fn notify(&mut self, method: &str) {
        let line = json!({ "jsonrpc": "2.0", "method": method }).to_string() + "\n";
        self.stdin.write_all(line.as_bytes()).await.unwrap();
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let line =
            json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params })
                .to_string()
                + "\n";
        self.stdin.write_all(line.as_bytes()).await.unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(10), self.stdout.next_line())
            .await
            .expect("an answer")
            .unwrap()
            .expect("a line");
        let reply: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["id"], self.next_id);
        reply
    }

    async fn call(&mut self, name: &str, arguments: Value) -> (String, bool) {
        let reply = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_string(),
            result["isError"].as_bool().unwrap(),
        )
    }
}

#[tokio::test]
async fn an_external_harness_works_a_ticket_through_mcp() {
    let Some(BusyWorker {
        dir,
        server,
        addr,
        mut socket,
        orchestrator,
        worker,
    }) = busy_worker("noted", true).await
    else {
        return;
    };

    // The worker works in its own worktree, on the ticket's branch.
    let worktree = dir.path().join("state/worktrees/0001");
    assert_eq!(
        server.sessions.get(&worker).unwrap().info().project,
        Some(worktree.to_string_lossy().to_string())
    );
    assert!(worktree.join("README.md").is_file());
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(ticket.contains("branch: ticket/0001"), "{ticket}");

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_composer"))
        .args([
            "mcp",
            "--session",
            &worker,
            "--parent",
            &format!("http://{addr}"),
        ])
        .env("COMPOSER_HOME", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut mcp = Mcp {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()).lines(),
        next_id: 0,
    };

    let init = mcp
        .request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" } }),
        )
        .await;
    assert_eq!(init["result"]["serverInfo"]["name"], "composer");
    mcp.notify("notifications/initialized").await;

    // The worker agent's tools, as a native worker gets them.
    let list = mcp.request("tools/list", json!({})).await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"open") && names.contains(&"finish"));
    assert!(!names.contains(&"spawn_worker"));

    let (ticket, failed) = mcp
        .call(
            "open",
            json!({ "url": "site://orchestration/tickets/0001" }),
        )
        .await;
    assert!(!failed, "{ticket}");
    assert!(ticket.contains("Take forever"));

    let (_, failed) = mcp
        .call(
            "finish",
            json!({ "status": "done", "summary": "did it over MCP" }),
        )
        .await;
    assert!(!failed);

    let events = until(&mut socket, 30, |e| {
        matches!(e, ServerMessage::MessageDone { session, message, .. }
            if *session == orchestrator && message.content == "noted")
    })
    .await;
    assert!(events.iter().any(|e| matches!(
        e,
        ServerMessage::WorkerStatus {
            state: WorkerState::Done,
            ..
        }
    )));
    assert!(events.iter().any(
        |e| matches!(e, ServerMessage::MessageDone { session, message, .. }
        if *session == orchestrator && message.content.contains("finished ticket 0001: done")
            && message.content.contains("did it over MCP")
            && message.content.contains("on branch `ticket/0001`"))
    ));
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(ticket.contains("status: done"));

    // Finished: the worker's tools are gone.
    let (output, failed) = mcp.call("open", json!({ "url": "site://home" })).await;
    assert!(
        failed && output.contains("not a running worker"),
        "{output}"
    );
    assert!(server.workers.running().is_empty());

    // The harness exiting closes stdin; the tool server stops with it.
    drop(mcp);
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("composer mcp exits")
        .unwrap();
    assert!(status.success());
}
