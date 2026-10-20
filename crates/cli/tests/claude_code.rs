//! The Claude Code harness against a fake `claude` (a shell script that speaks stream-json and
//! finishes through a real `composer mcp`): project switch, login check, cleared environment,
//! the run's flags, folder and ticket fields, a run without `finish`, resuming, a usage limit.

mod support;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use composer_protocol::WorkerState;
use composer_server::session::Session;
use composer_server::{Home, Server, init_home};
use support::{Reply, Scripts, fake_model};

const FAKE_CLAUDE: &str = r#"#!/usr/bin/env bash
here="$(cd "$(dirname "$0")" && pwd)"
if [ "$1" = auth ]; then
  if [ -e "$here/logged-out" ]; then echo '{"loggedIn": false}'; else echo '{"loggedIn": true, "authMethod": "claude.ai"}'; fi
  exit 0
fi
printf '%s\n' "$@" > "$here/args.txt"
env > "$here/env.txt"
pwd > "$here/cwd.txt"
mode="$(cat "$here/mode")"
mcp=""; prev=""
for a in "$@"; do [ "$prev" = "--mcp-config" ] && mcp="$a"; prev="$a"; done
pick() { printf '%s' "$mcp" | sed -n "s/.*$1\"\([^\"]*\)\".*/\1/p"; }
sid=$(pick '"--session",')
parent=$(pick '"--parent",')
exe=$(pick '"command":')
home=$(pick '"COMPOSER_HOME":')
echo '{"type":"system","subtype":"init","session_id":"x","model":"fake-model"}'
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"On it."},{"type":"tool_use","id":"t1","name":"mcp__composer__open","input":{"url":"site://home"}}]}}'
echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Home"}]}}'
case "$mode" in
  finish)
    { echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}'
      echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"finish","arguments":{"status":"done","summary":"the fake did it"}}}'
      sleep 1; } | COMPOSER_HOME="$home" "$exe" mcp --session "$sid" --parent "$parent" > "$here/mcp.txt"
    echo '{"type":"result","subtype":"success","is_error":false,"result":"done"}' ;;
  limit)
    echo '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":4102444800}}'
    echo '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"usage limit reached","api_error_status":429}'
    exit 1 ;;
  ask)
    # Asks to run something outside the allowlist, then finishes.
    { echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}'
      echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"permission","arguments":{"tool_name":"Bash","input":{"command":"python3 make.py","description":"Build"},"tool_use_id":"t2"}}}'
      sleep 4
      echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"finish","arguments":{"status":"done","summary":"asked"}}}'
      sleep 1; } | COMPOSER_HOME="$home" "$exe" mcp --session "$sid" --parent "$parent" > "$here/mcp.txt"
    echo '{"type":"result","subtype":"success","is_error":false,"result":"done"}' ;;
  silent)
    echo '{"type":"result","subtype":"success","is_error":false,"result":"I could not do it"}' ;;
  slow)
    sleep 30 ;;
esac
"#;

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}

fn ticket(dir: &Path, id: &str, project: &Path) {
    std::fs::create_dir_all(dir.join("sites/orchestration/tickets")).unwrap();
    std::fs::write(
        dir.join(format!("sites/orchestration/tickets/{id}.md")),
        format!(
            "---\ntitle: Ticket {id}\ntype: ticket\nid: \"{id}\"\nstatus: open\nagent: worker\nproject: {}\n---\n# Ticket {id}\n\n## Task\nSomething.\n\n## Result\n",
            project.display()
        ),
    )
    .unwrap();
}

/// Waits until ticket `id` has `status: <status>`; returns its text.
async fn until_status(dir: &Path, id: &str, status: &str) -> String {
    let file = dir.join(format!("sites/orchestration/tickets/{id}.md"));
    for _ in 0..200 {
        let text = std::fs::read_to_string(&file).unwrap();
        if text.contains(&format!("status: {status}")) {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!(
        "ticket {id} never got status {status}:\n{}",
        std::fs::read_to_string(&file).unwrap()
    );
}

/// Waits until ticket `id` contains `needle`.
async fn until_field(dir: &Path, id: &str, needle: &str, last: &str) -> String {
    let file = dir.join(format!("sites/orchestration/tickets/{id}.md"));
    for _ in 0..200 {
        let text = std::fs::read_to_string(&file).unwrap();
        if text.contains(needle) {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("ticket {id} never got {needle}:\n{last}");
}

async fn spawn(server: &Arc<Server>, parent: &Arc<Session>, id: &str) -> Result<String, String> {
    server
        .workers
        .spawn(server, parent, id, None, Some("claude-code"))
        .await
        .map(|(worker, _)| worker)
}

#[tokio::test]
async fn claude_code_works_tickets() {
    let base_url = fake_model(Scripts {
        orchestrator: VecDeque::from(vec![Reply::Text("noted"); 10]),
        worker: VecDeque::new(),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let home = Home::new(&root);
    init_home(&home).await.unwrap();
    assert!(
        root.join("templates/harness/claude-code/.claude-plugin/plugin.json")
            .is_file(),
        "init installs the skills plugin"
    );

    let (project, other) = (root.join("project"), root.join("other"));
    for folder in [&project, &other] {
        std::fs::create_dir_all(folder).unwrap();
        std::fs::write(folder.join("README.md"), "# p\n").unwrap();
        git(folder, &["init", "-q", "-b", "main"]);
        git(folder, &["add", "."]);
        git(
            folder,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "init",
            ],
        );
    }
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("claude");
    std::fs::write(&fake, FAKE_CLAUDE).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(
        home.config_file(),
        format!(
            "[provider]\nbase_url = \"{base_url}\"\n\n[projects.\"{}\"]\nexternal_harnesses = [\"claude-code\"]\nallowed_tools = [\"Bash(cargo test:*)\"]\nmodels = [\"claude-code:sonnet\"]\n\n[claude_code]\ncommand = \"{}\"\n",
            project.display(),
            fake.display()
        ),
    )
    .unwrap();
    // SAFETY: set before any server or thread reads them; the only test in this binary.
    unsafe {
        std::env::set_var("COMPOSER_WORKER_EXE", env!("CARGO_BIN_EXE_composer"));
        // Must not reach the CLI: it would bill the API instead of the subscription.
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test-not-for-claude");
    }
    let server = Server::start(home).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let serving = server.clone();
    tokio::spawn(async move { serving.serve(listener).await });
    for _ in 0..50 {
        if server.listen_addr().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let orchestrator = server
        .sessions
        .create("orchestrator", Some(project.display().to_string()))
        .unwrap();

    // A project that doesn't allow it.
    ticket(&root, "0001", &other);
    let refused = spawn(&server, &orchestrator, "0001").await.unwrap_err();
    assert!(
        refused.contains("does not allow the `claude-code` harness"),
        "{refused}"
    );

    // Not logged in: the error, and the ticket stays open.
    std::fs::write(bin.join("logged-out"), "").unwrap();
    ticket(&root, "0002", &project);
    let refused = spawn(&server, &orchestrator, "0002").await.unwrap_err();
    assert!(refused.contains("not logged in"), "{refused}");
    assert!(
        std::fs::read_to_string(root.join("sites/orchestration/tickets/0002.md"))
            .unwrap()
            .contains("status: open")
    );
    std::fs::remove_file(bin.join("logged-out")).unwrap();

    // Ticket attachments: files from this project's sessions reach it; another project's don't.
    let mine = composer_server::attachments::save(
        &Home::new(&root),
        &server.config.attachments,
        orchestrator.id(),
        "spec.md",
        b"# spec",
    )
    .unwrap();
    let elsewhere = server
        .sessions
        .create("default", Some(other.display().to_string()))
        .unwrap();
    let theirs = composer_server::attachments::save(
        &Home::new(&root),
        &server.config.attachments,
        elsewhere.id(),
        "secret.md",
        b"# secret",
    )
    .unwrap();
    let file = root.join("sites/orchestration/tickets/0002.md");
    let text = std::fs::read_to_string(&file).unwrap().replace(
        "agent: worker\n",
        &format!(
            "agent: worker\nattachments: [\"{}/{}\", \"{}/{}\"]\n",
            orchestrator.id(),
            mine.file,
            elsewhere.id(),
            theirs.file
        ),
    );
    std::fs::write(&file, text).unwrap();

    // A run that finishes through composer mcp.
    std::fs::write(bin.join("mode"), "finish").unwrap();
    let worker = spawn(&server, &orchestrator, "0002").await.unwrap();
    let text = until_status(&root, "0002", "done").await;
    // The exit is recorded once the process has ended, after `finish`.
    let text = until_field(&root, "0002", "exit: ", &text).await;
    for field in [
        "harness: claude-code",
        "branch: ticket/0002",
        "model: fake-model",
        "exit: exit 0",
        "harness_session: ",
    ] {
        assert!(text.contains(field), "{field} in:\n{text}");
    }
    let env = std::fs::read_to_string(bin.join("env.txt")).unwrap();
    assert!(
        !env.contains("ANTHROPIC_API_KEY") && !env.contains("COMPOSER_"),
        "{env}"
    );
    assert!(env.contains("HOME="));
    let cwd = std::fs::read_to_string(bin.join("cwd.txt")).unwrap();
    assert_eq!(
        Path::new(cwd.trim()),
        root.join("state/worktrees/0002"),
        "it works in the ticket's worktree"
    );
    let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
    for arg in [
        "stream-json",
        "--strict-mcp-config",
        "--session-id",
        "--plugin-dir",
        "Bash(cargo test:*)",
    ] {
        assert!(args.contains(arg), "{arg} in:\n{args}");
    }
    assert!(args.contains(
        "--permission-prompt-tool\nmcp__composer__permission\n--permission-prompts\nhost\n"
    ));
    let env = std::fs::read_to_string(bin.join("env.txt")).unwrap();
    assert!(env.contains("MCP_TOOL_TIMEOUT=3600000"), "{env}");
    assert!(
        args.contains(&format!(
            "--add-dir\n{}",
            root.join("state/sessions")
                .join(orchestrator.id())
                .join("attachments")
                .display()
        )),
        "{args}"
    );
    assert!(
        args.contains(&mine.file) && !args.contains(&theirs.file),
        "{args}"
    );
    // The project's models pick the model.
    assert!(args.contains("--model\nsonnet\n"), "{args}");
    let events =
        std::fs::read_to_string(root.join(format!("state/sessions/{worker}/events.jsonl")))
            .unwrap();
    assert!(events.contains(r#""name":"open""#), "{events}");
    assert!(events.contains("On it."));
    assert_eq!(
        server.sessions.get(&worker).unwrap().info().worker,
        Some(WorkerState::Done)
    );

    // Outside the allowlist: asked in the worker's session; `always` remembers it for the
    // project, so the next run has it in --allowedTools and isn't asked.
    std::fs::write(bin.join("mode"), "ask").unwrap();
    ticket(&root, "0008", &project);
    let worker = spawn(&server, &orchestrator, "0008").await.unwrap();
    let request = loop {
        if let Some(composer_protocol::ServerMessage::ApprovalRequest {
            request,
            session,
            kind,
            title,
            body,
            rule,
            ..
        }) = server.pending_approvals.all().pop()
        {
            assert_eq!(session, worker);
            assert_eq!(kind, composer_protocol::ApprovalKind::Tool);
            assert_eq!(title, "Claude Code: Bash: python3 make.py");
            assert_eq!(body, "python3 make.py\n\n# Build");
            assert_eq!(rule.as_deref(), Some("Bash(python3:*)"));
            break request;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(
        server.sessions.get(&worker).unwrap().info().state,
        composer_protocol::TurnState::AwaitingApproval
    );
    assert!(
        server
            .pending_approvals
            .resolve(&request, composer_protocol::ApprovalDecision::Always)
    );
    until_status(&root, "0008", "done").await;
    let mcp = std::fs::read_to_string(bin.join("mcp.txt")).unwrap();
    assert!(mcp.contains(r#"\"behavior\":\"allow\""#), "{mcp}");
    ticket(&root, "0009", &project);
    spawn(&server, &orchestrator, "0009").await.unwrap();
    until_status(&root, "0009", "done").await;
    let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
    assert!(args.contains("Bash(python3:*)"), "{args}");
    let mcp = std::fs::read_to_string(bin.join("mcp.txt")).unwrap();
    assert!(
        mcp.contains(r#"\"behavior\":\"allow\""#),
        "not asked again: {mcp}"
    );
    assert!(server.pending_approvals.all().is_empty());

    // A run that ends without finish fails, with what it last said; a message resumes it.
    std::fs::write(bin.join("mode"), "silent").unwrap();
    ticket(&root, "0003", &project);
    let worker = spawn(&server, &orchestrator, "0003").await.unwrap();
    until_status(&root, "0003", "failed").await;
    let session = server.sessions.get(&worker).unwrap();
    assert!(server.workers.resumable(&server, &session));
    std::fs::write(bin.join("mode"), "finish").unwrap();
    server
        .workers
        .resume(&server, &session, "try again".into(), vec![])
        .await
        .unwrap();
    until_status(&root, "0003", "done").await;
    let args = std::fs::read_to_string(bin.join("args.txt")).unwrap();
    assert!(
        args.contains("--resume") && args.contains("try again"),
        "{args}"
    );

    // Turned off for the project: the running ticket pauses, new ones are refused; turned
    // back on, it goes on by itself.
    let project_key = project.display().to_string();
    std::fs::write(bin.join("mode"), "slow").unwrap();
    ticket(&root, "0006", &project);
    spawn(&server, &orchestrator, "0006").await.unwrap();
    until_field(&root, "0006", "model: ", "").await;
    server
        .workers
        .switch(&server, &project_key, "claude-code", false)
        .await
        .unwrap();
    let text = until_status(&root, "0006", "paused").await;
    assert!(text.contains("paused_reason: switch"), "{text}");
    ticket(&root, "0007", &project);
    let refused = spawn(&server, &orchestrator, "0007").await.unwrap_err();
    assert!(refused.contains("turned off for this project"), "{refused}");
    let switches = server.workers.switches(&server, &project_key);
    assert!(
        switches
            .iter()
            .any(|h| h.entry == "claude-code" && h.allowed && !h.on)
    );
    std::fs::write(bin.join("mode"), "finish").unwrap();
    server
        .workers
        .switch(&server, &project_key, "claude-code", true)
        .await
        .unwrap();
    until_status(&root, "0006", "done").await;

    // Shutting down pauses a running ticket (without telling the orchestrator).
    std::fs::write(bin.join("mode"), "slow").unwrap();
    spawn(&server, &orchestrator, "0007").await.unwrap();
    until_field(&root, "0007", "model: ", "").await;
    server.workers.shutdown().await;
    let text = until_status(&root, "0007", "paused").await;
    assert!(text.contains("paused_reason: shutdown"), "{text}");
    assert!(server.workers.running().is_empty());

    // A usage limit pauses the ticket and Claude Code, instead of failing.
    std::fs::write(bin.join("mode"), "limit").unwrap();
    ticket(&root, "0004", &project);
    let worker = spawn(&server, &orchestrator, "0004").await.unwrap();
    until_status(&root, "0004", "paused").await;
    assert_eq!(
        server.sessions.get(&worker).unwrap().info().worker,
        Some(WorkerState::Paused)
    );
    ticket(&root, "0005", &project);
    let refused = spawn(&server, &orchestrator, "0005").await.unwrap_err();
    assert!(refused.contains("paused until"), "{refused}");
}
