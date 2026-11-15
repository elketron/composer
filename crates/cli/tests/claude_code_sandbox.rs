//! The Claude Code harness in the sandbox (ticket 0158), with a fake claude that checks its
//! world from inside: its ticket worktree and its login, the workspace own ~/.claude; not the
//! main checkout, another workspace, the library or composer state. The login (0202): a stored
//! token wins, else the user own ~/.claude/.credentials.json is mounted read-write so an OAuth
//! refresh writes through - their other shared files stay read-only, and memory, history and
//! sessions stay per workspace. With neither, nothing starts. It commits in its worktree and
//! finishes through a real composer mcp, which gets the workspace token from its env (the sandbox
//! has no state/).
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
checks = {}
values = {}

def check(name, ok):
    checks[name] = bool(ok)

def value(name, v):
    values[name] = v

def read(path):
    try:
        return open(path).read()
    except OSError:
        return None

e = os.path.exists
check("no api key", "ANTHROPIC_API_KEY" not in os.environ)
value("token env", os.environ.get("CLAUDE_CODE_OAUTH_TOKEN", ""))
# The login: the user own file, mounted read-write over the workspace own ~/.claude, so an OAuth
# refresh writes through. Their other shared files stay read-only.
login = paths["login"]
value("login content", read(login))
check("login writable", os.access(login, os.W_OK))
if os.access(login, os.W_OK):
    open(login, "w").write("refreshed inside\n")
shared = os.path.join(os.environ["HOME"], ".claude", "CLAUDE.md")
value("shared content", read(shared))
check("shared read-only", not os.access(shared, os.W_OK))
try:
    open(os.path.join(os.environ["HOME"], ".claude", "own.txt"), "w").write("written inside\n")
    check("own .claude writable", True)
except OSError:
    check("own .claude writable", False)
value("cwd", os.getcwd())
check("worktree files", e("README.md"))
check("main checkout hidden", not e(os.path.join(paths["project"], "README.md")))
for name in ["b", "b_claude", "library", "home_state", "ws_state"]:
    check(name + " hidden", not e(paths[name]))
with open("CHANGE", "w") as f:
    f.write("change\n")
ok = subprocess.run(["git", "add", "CHANGE"]).returncode == 0
ok = ok and subprocess.run(
    ["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "inside"]
).returncode == 0
check("commit", ok)
with open(os.path.join(paths["out"], "report.json"), "w") as f:
    json.dump({"checks": checks, "values": values}, f)

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
send({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
      "params": {"name": "finish", "arguments": {"status": "done", "summary": "sandboxed"}}})
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

/// The report of a run, once it has written one (20 s); it is removed, so the next run starts
/// from nothing.
async fn wait_report(out: &Path) -> Option<serde_json::Value> {
    let file = out.join("report.json");
    for _ in 0..200 {
        if let Ok(text) = std::fs::read_to_string(&file) {
            let _ = std::fs::remove_file(&file);
            return serde_json::from_str(&text).ok();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

/// The checks of a report that failed.
fn failed(report: &serde_json::Value) -> Vec<String> {
    report["checks"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, ok)| *ok != &json!(true))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The ticket file, once finish has landed through the real mcp (20 s).
async fn done(file: &Path) -> bool {
    for _ in 0..200 {
        if std::fs::read_to_string(file)
            .unwrap_or_default()
            .contains("status: done")
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[tokio::test]
async fn claude_code_runs_sandboxed_on_the_stored_token_or_the_shared_login() {
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
            "[provider]\nbase_url = \"{base_url}\"\n\n[projects.\"{}\"]\nexternal_harnesses =              [\"claude-code\"]\n\n[claude_code]\ncommand = \"{}\"\n\n[sandbox]\nbinds_ro =              [\"{}\"]\nbinds_rw = [\"{}\"]\n",
            project.display(),
            fake.display(),
            bin.display(),
            out.display(),
        ),
    )
    .unwrap();
    let token = "sk-ant-oat01-a-test-token-for-the-sandbox";
    // The user own ~/.claude, in a home of its own so the test controls it: the credential file is
    // the one file of theirs a run may write through; the shared memory stays read-only. What
    // workspace b keeps in its own .claude must stay hidden from a run of workspace a.
    let user_home = root.join("userhome");
    std::fs::create_dir_all(user_home.join(".claude")).unwrap();
    let login = user_home.join(".claude/.credentials.json");
    std::fs::write(&login, "the-user-login\n").unwrap();
    std::fs::write(user_home.join(".claude/CLAUDE.md"), "the-user-memory\n").unwrap();
    std::fs::create_dir_all(b.state().join("claude/.claude")).unwrap();
    std::fs::write(b.state().join("claude/.claude/CLAUDE.md"), "not yours\n").unwrap();
    std::fs::write(
        bin.join("paths.json"),
        json!({
            "out": out,
            "project": project,
            "b": b.root(),
            "b_claude": b.state().join("claude/.claude/CLAUDE.md"),
            "library": home.library(),
            "home_state": home.state(),
            "ws_state": a.state().join("token"),
            "login": login,
        })
        .to_string(),
    )
    .unwrap();
    // SAFETY: set before any server or thread reads them; the only test in this binary.
    unsafe {
        std::env::set_var("COMPOSER_WORKER_EXE", env!("CARGO_BIN_EXE_composer"));
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test-not-for-claude");
        std::env::set_var("HOME", &user_home);
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
    for id in ["0001", "0002", "0003"] {
        std::fs::write(
            tickets.join(format!("{id}.md")),
            format!(
                "---\ntitle: Ticket {id}\ntype: ticket\nid: \"{id}\"\nstatus: open\nagent: default\nproject: {}\n---\n# Ticket {id}\n\n## Task\nSomething.\n\n## Result\n",
                project.display()
            ),
        )
        .unwrap();
    }
    let spawn = |id: &'static str| {
        let (server, orchestrator) = (server.clone(), orchestrator.clone());
        async move {
            server
                .workers
                .spawn(
                    &server,
                    &orchestrator,
                    &composer_server::worker::ticket::TicketId::parse(id).unwrap(),
                    Some("claude-code"),
                )
                .await
        }
    };

    // No stored token: the run goes ahead on the login of the user, mounted read-write so an
    // OAuth refresh writes through to the real file. The shared memory is the users, read-only;
    // the workspace own ~/.claude is writable; nothing of workspace b shows.
    spawn("0001").await.unwrap();
    let r = wait_report(&out).await.expect("the first run reported");
    assert_eq!(failed(&r), Vec::<String>::new(), "{r:#}");
    assert_eq!(r["values"]["token env"], json!(""), "no stored token yet");
    assert_eq!(
        r["values"]["login content"],
        json!("the-user-login\n"),
        "the mount is the login file of the user"
    );
    assert_eq!(r["values"]["shared content"], json!("the-user-memory\n"));
    assert_eq!(r["values"]["cwd"], json!(a.worktrees().join("0001")));
    assert_eq!(
        std::fs::read_to_string(&login).unwrap(),
        "refreshed inside\n"
    );
    assert_eq!(
        std::fs::read_to_string(a.state().join("claude/.claude/own.txt")).unwrap(),
        "written inside\n"
    );
    assert!(!user_home.join(".claude/own.txt").exists());
    assert_eq!(
        std::fs::read_to_string(b.state().join("claude/.claude/CLAUDE.md")).unwrap(),
        "not yours\n"
    );
    assert!(done(&tickets.join("0001.md")).await, "{r:#}");

    // A stored token wins when there is one; the login stays mounted for the refresh path.
    ClaudeToken::new(token).unwrap().save(&home).unwrap();
    spawn("0002").await.unwrap();
    let r = wait_report(&out).await.expect("the second run reported");
    assert_eq!(failed(&r), Vec::<String>::new(), "{r:#}");
    assert_eq!(
        r["values"]["token env"],
        json!(token),
        "a stored token wins"
    );
    assert_eq!(r["values"]["cwd"], json!(a.worktrees().join("0002")));
    assert!(done(&tickets.join("0002.md")).await, "{r:#}");

    // Neither a stored token nor a login file to mount: refused, nothing starts.
    std::fs::remove_file(ClaudeToken::file(&home)).unwrap();
    std::fs::remove_file(&login).unwrap();
    let refused = spawn("0003").await.unwrap_err();
    assert!(refused.contains("claude setup-token"), "{refused}");
    assert!(refused.contains("auth login"), "{refused}");
    assert!(!out.join("report.json").exists());

    // The commits of both runs are on their own ticket branches.
    for branch in ["ticket/0001", "ticket/0002"] {
        let log = std::process::Command::new("git")
            .arg("-C")
            .arg(&project)
            .args(["log", "--oneline", branch])
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&log.stdout).contains("inside"),
            "{branch}"
        );
    }
}
