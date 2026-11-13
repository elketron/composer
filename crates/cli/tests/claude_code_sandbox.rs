//! The Claude Code harness in the sandbox (ticket 0158), with a fake `claude` that checks its
//! world from inside: its ticket's worktree and the home's token, its workspace's own `~/.claude`;
//! not the main checkout, another workspace, the library, composer's state or the real Claude
//! Code credentials. It commits in its worktree and finishes through a real `composer mcp`, which
//! gets the workspace token from its env (the sandbox has no `state/`). Without a stored token,
//! nothing starts.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use composer_server::harness::ClaudeToken;
use composer_server::{Home, Server, WorkspaceName, init_home, init_workspace};
use serde_json::json;
use support::{Reply, Scripts, fake_model};

const FAKE_CLAUDE: &str = r#"#!/usr/bin/env python3
import json, os, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
paths = json.load(open(os.path.join(here, "paths.json")))
args = sys.argv[1:]
report = []

def check(name, ok):
    report.append(("ok " if ok else "FAIL ") + name)

e = os.path.exists
check("token", os.environ.get("CLAUDE_CODE_OAUTH_TOKEN") == paths["token"])
check("no api key", "ANTHROPIC_API_KEY" not in os.environ)
check("cwd", os.getcwd() == paths["worktree"])
check("worktree files", e("README.md"))
check("main checkout hidden", not e(os.path.join(paths["project"], "README.md")))
for name in ["b", "library", "home_state", "ws_state", "credentials"]:
    check(name + " hidden", not e(paths[name]))
claude = os.path.join(os.environ["HOME"], ".claude")
with open(os.path.join(claude, "probe"), "w") as f:
    f.write("written inside\n")
check("own .claude", True)
with open("CHANGE", "w") as f:
    f.write("change\n")
ok = subprocess.run(["git", "add", "CHANGE"]).returncode == 0
ok = ok and subprocess.run(
    ["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "inside"]
).returncode == 0
check("commit", ok)
with open(os.path.join(paths["out"], "report.txt"), "w") as f:
    f.write("\n".join(report) + "\n")

mcp = json.loads(args[args.index("--mcp-config") + 1])["mcpServers"]["composer"]
env = dict(os.environ)
env.update(mcp["env"])
proc = subprocess.Popen(
    [mcp["command"]] + mcp["args"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env, text=True
)
def send(message):
    proc.stdin.write(json.dumps(message) + "\n")
    proc.stdin.flush()
    return json.loads(proc.stdout.readline())
send({"jsonrpc": "2.0", "id": 1, "method": "initialize",
      "params": {"protocolVersion": "2025-06-18", "capabilities": {}}})
reply = send({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
              "params": {"name": "finish", "arguments": {"status": "done", "summary": "sandboxed"}}})
with open(os.path.join(paths["out"], "mcp.txt"), "w") as f:
    f.write(json.dumps(reply))
proc.stdin.close()
proc.wait()
print(json.dumps({"type": "system", "subtype": "init", "session_id": "x", "model": "fake-model"}))
print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": "done"}))
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

#[tokio::test]
async fn claude_code_runs_sandboxed_with_its_own_login_and_home() {
    let base_url = fake_model(Scripts {
        orchestrator: VecDeque::from(vec![Reply::Text("noted"); 10]),
        worker: VecDeque::new(),
        ..Scripts::default()
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let home = Home::new(root.join("composer"));
    init_home(&home).await.unwrap();
    let b = home.workspace(&WorkspaceName::new("b").unwrap());
    init_workspace(&b).await.unwrap();
    let a = home.workspace(&WorkspaceName::default_workspace());

    let project = root.join("projects/p");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("README.md"), "# p\n").unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["add", "."]);
    git(
        &project,
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
    let (bin, out) = (root.join("bin"), root.join("out"));
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let fake = bin.join("claude");
    std::fs::write(&fake, FAKE_CLAUDE).unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::write(
        a.config_file(),
        format!(
            "[provider]\nbase_url = \"{base_url}\"\n\n[projects.\"{}\"]\nexternal_harnesses = \
             [\"claude-code\"]\n\n[claude_code]\ncommand = \"{}\"\n\n[sandbox]\nbinds_ro = \
             [\"{}\"]\nbinds_rw = [\"{}\"]\n",
            project.display(),
            fake.display(),
            bin.display(),
            out.display(),
        ),
    )
    .unwrap();
    let token = "sk-ant-oat01-a-test-token-for-the-sandbox";
    let user_home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
    let worktree = a.worktrees().join("0001");
    std::fs::write(
        bin.join("paths.json"),
        json!({
            "token": token,
            "out": out,
            "worktree": worktree,
            "project": project,
            "b": b.root(),
            "library": home.library(),
            "home_state": home.state(),
            "ws_state": a.state().join("token"),
            "credentials": user_home.join(".claude/.credentials.json"),
        })
        .to_string(),
    )
    .unwrap();
    // SAFETY: set before any server or thread reads them; the only test in this binary.
    unsafe {
        std::env::set_var("COMPOSER_WORKER_EXE", env!("CARGO_BIN_EXE_composer"));
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test-not-for-claude");
    }
    let server = Server::start(a.clone()).await.unwrap();
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
    let tickets = a.sites().join("orchestration/tickets");
    std::fs::create_dir_all(&tickets).unwrap();
    std::fs::write(
        tickets.join("0001.md"),
        format!(
            "---\ntitle: Ticket 0001\ntype: ticket\nid: \"0001\"\nstatus: open\nagent: default\n\
             project: {}\n---\n# Ticket 0001\n\n## Task\nSomething.\n\n## Result\n",
            project.display()
        ),
    )
    .unwrap();
    let spawn = || async {
        server
            .workers
            .spawn(
                &server,
                &orchestrator,
                &composer_server::worker::ticket::TicketId::parse("0001").unwrap(),
                Some("claude-code"),
            )
            .await
            .map(|(worker, _)| worker)
    };

    // No token yet: refused before anything starts.
    let refused = spawn().await.unwrap_err();
    assert!(refused.contains("claude setup-token"), "{refused}");
    assert!(!out.join("report.txt").exists());

    ClaudeToken::new(token).unwrap().save(&home).unwrap();
    spawn().await.unwrap();
    let file = tickets.join("0001.md");
    let mut text = String::new();
    for _ in 0..200 {
        text = std::fs::read_to_string(&file).unwrap();
        if text.contains("status: done") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let report = std::fs::read_to_string(out.join("report.txt")).unwrap_or_default();
    let mcp = std::fs::read_to_string(out.join("mcp.txt")).unwrap_or_default();
    let logs: String = walk(&a.state())
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .map(|p| std::fs::read_to_string(p).unwrap_or_default())
        .collect();
    assert!(
        text.contains("status: done"),
        "{text}\nreport:\n{report}\nmcp:\n{mcp}\nlogs:\n{logs}"
    );
    assert!(
        !report.contains("FAIL") && report.lines().count() == 12,
        "{report}"
    );
    // Its .claude is the workspace's, on the host.
    assert_eq!(
        std::fs::read_to_string(a.state().join("claude/.claude/probe")).unwrap(),
        "written inside\n"
    );
    assert!(!user_home.join(".claude/probe").exists());
    // The commit is on the ticket's branch.
    let log = std::process::Command::new("git")
        .arg("-C")
        .arg(&project)
        .args(["log", "--oneline", "ticket/0001"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).contains("inside"));
}

/// Every file under `dir`.
fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
