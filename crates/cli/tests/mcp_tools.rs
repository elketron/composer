//! `composer mcp` serves a session's tools over stdio: an MCP client lists them and calls one,
//! and the tool runs in that session on the server.

mod support;

use std::process::Stdio;
use std::time::Duration;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::{Home, Server, init_home};
use serde_json::{Value, json};
use support::{fake_model, send, until};
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
async fn a_client_uses_a_sessions_tools_through_mcp() {
    let base_url = fake_model().await;
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    std::fs::write(
        home.config_file(),
        format!("[provider]\nbase_url = \"{base_url}\"\n"),
    )
    .unwrap();
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
            kind: composer_protocol::RunKind::Agentic,
            model: None,
            site: None,
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
        panic!("{reply:?}")
    };

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_composer"))
        .args([
            "mcp",
            "--session",
            &session.id,
            "--server",
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

    // The session's agent's tools.
    let list = mcp.request("tools/list", json!({})).await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"open") && names.contains(&"search"),
        "{names:?}"
    );

    let (home_page, failed) = mcp.call("open", json!({ "url": "site://home" })).await;
    assert!(!failed, "{home_page}");
    let (output, failed) = mcp.call("open", json!({ "url": "site://nope" })).await;
    assert!(failed, "{output}");

    // The client exiting closes stdin; the tool server stops with it.
    drop(mcp);
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("composer mcp exits")
        .unwrap();
    assert!(status.success());
}
