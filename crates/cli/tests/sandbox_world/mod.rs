//! A home with two workspaces for the sandbox review (ticket 0160), and one probe the three kinds
//! of spawn run: a recipe, a stdio MCP server and a Claude Code run (`claude` is the probe). It
//! tries what a spawn of workspace `default` must not reach and what it must, and writes its
//! report to `out/<kind>.json`.

#![allow(
    dead_code,
    reason = "shared by two test binaries, each uses part of it"
)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use composer_server::actions::{Justfile, RecipeEnv, run_recipe};
use composer_server::harness::ClaudeToken;
use composer_server::worker::ticket::TicketId;
use composer_server::{Home, Server, WorkspaceHome, WorkspaceName, init_home, init_workspace};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::support::{Reply, Scripts, fake_model};

/// `probe.py <kind>`: checks, then (as `mcp`) serves MCP with no tools, or (as `claude`) commits
/// and finishes through `composer mcp`, like a run.
pub const PROBE: &str = r#"#!/usr/bin/env python3
import json, os, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
paths = json.load(open(os.path.join(here, "paths.json")))
kind = "claude" if os.path.basename(sys.argv[0]) == "claude" else sys.argv[1]
report = {}

def check(name, ok):
    report[name] = bool(ok)

def can_write(folder):
    try:
        with open(os.path.join(folder, "from-" + kind), "w") as f:
            f.write("x")
        return True
    except OSError:
        return False

for name, path in paths["hidden"].items():
    check("hidden: " + name, not os.path.exists(path))
# Tried; whether they reached the host is checked outside (a write may land on the sandbox's own
# tmpfs root, which is fine).
for folder in paths["unwritable"].values():
    can_write(folder)
# A Claude Code run's worktree lives in a's state: that much of it, nothing else.
state = paths["a_state"]
# (A refused write may have landed on the sandbox's own skeleton of it: `from-*`.)
shown = sorted(n for n in os.listdir(state) if not n.startswith("from-")) if os.path.exists(state) else []
check("a's state: only the run's own parts", shown == (["worktrees"] if kind == "claude" else []))
if kind == "claude":
    check("a's worktrees: only its own", os.listdir(os.path.join(state, "worktrees")) == ["0001"])
    check("own ~/.claude writable", can_write(os.path.join(os.environ["HOME"], ".claude")))
# The user's own files a spawn is lent are read-only (asked, never written: they are real).
lent = ["/usr/bin", "/etc", paths["home_templates"], paths["ws_templates"]] + [os.path.join(os.environ["HOME"], p) for p in [".cargo/bin", ".rustup", ".gitconfig"]]
if kind == "claude":
    lent += [os.path.join(os.environ["HOME"], ".claude", n) for n in ["CLAUDE.md", "settings.json", "skills", "agents", "commands", "plugins"]]
for path in lent:
    if os.path.exists(path):
        check("read-only: " + path, not os.access(path, os.W_OK))
check("own sites writable", can_write(paths["sites"]))
check("own /tmp writable", can_write("/tmp"))
check("host /tmp hidden", not os.path.exists(paths["host_tmp"]))
for name in paths["home_entries"] + (paths["home_entries_no_claude"] if kind != "claude" else []):
    check("hidden: ~/" + name, not os.path.exists(os.path.join(os.environ["HOME"], name)))
for ns in ["pid", "ipc", "uts", "cgroup"]:
    check("own " + ns + " namespace", os.readlink("/proc/self/ns/" + ns) != paths["ns"][ns])
check("hostname", os.uname().nodename == "sandbox")
if kind == "claude":
    check("token in env", os.environ.get("CLAUDE_CODE_OAUTH_TOKEN") == paths["token"])
    check("cwd is the worktree", os.getcwd() == paths["worktree"])
    check("main checkout hidden", not os.path.exists(os.path.join(paths["project"], "README.md")))
    check("worktree writable", can_write(os.getcwd()))
else:
    check("project hidden", not os.path.exists(os.path.join(paths["project"], "README.md")))

with open(os.path.join(paths["out"], kind + ".json"), "w") as f:
    json.dump(report, f)

if kind == "mcp":
    for line in sys.stdin:
        m = json.loads(line)
        if m.get("id") is None:
            continue
        r = {"protocolVersion": "2025-06-18", "capabilities": {}, "serverInfo": {"name": "p", "version": "1"}} \
            if m["method"] == "initialize" else {"tools": []}
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": r}) + "\n")
        sys.stdout.flush()
elif kind == "claude":
    args = sys.argv[1:]
    subprocess.run(["git", "add", "-A"])
    subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "probe"])
    mcp = json.loads(args[args.index("--mcp-config") + 1])["mcpServers"]["composer"]
    env = dict(os.environ)
    env.update(mcp["env"])
    proc = subprocess.Popen([mcp["command"]] + mcp["args"], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, env=env, text=True)
    def send(message):
        proc.stdin.write(json.dumps(message) + "\n")
        proc.stdin.flush()
        return json.loads(proc.stdout.readline())
    send({"jsonrpc": "2.0", "id": 1, "method": "initialize",
          "params": {"protocolVersion": "2025-06-18", "capabilities": {}}})
    send({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
          "params": {"name": "finish", "arguments": {"status": "done", "summary": "probed"}}})
    proc.stdin.close()
    proc.wait()
    print(json.dumps({"type": "system", "subtype": "init", "session_id": "x", "model": "fake"}))
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": "done"}))
"#;

pub const TOKEN: &str = "sk-ant-oat01-a-test-token-for-the-review";

pub struct World {
    pub dir: tempfile::TempDir,
    pub home: Home,
    pub a: WorkspaceHome,
    pub b: WorkspaceHome,
    pub project: PathBuf,
    pub out: PathBuf,
    /// Folders no spawn may write to (checked on the host: `from-<kind>` must not appear).
    pub unwritable: Vec<PathBuf>,
    pub server: std::sync::Arc<Server>,
    /// Gone with the world: a marker in the host's `/tmp`.
    _host_tmp: tempfile::TempDir,
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}

fn ns(name: &str) -> String {
    std::fs::read_link(format!("/proc/self/ns/{name}"))
        .unwrap()
        .display()
        .to_string()
}

/// The two workspaces, a's project (a git repo), a's config with the probe as a recipe, an MCP
/// server and the Claude Code command, the login token stored; a's server started and serving.
pub async fn world() -> World {
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
    std::fs::write(b.sites().join("home/secret.md"), "b's\n").unwrap();
    let a = home.workspace(&WorkspaceName::default_workspace());

    let project = root.join("projects/p");
    let theirs = root.join("projects/theirs");
    for p in [&project, &theirs] {
        std::fs::create_dir_all(p).unwrap();
        std::fs::write(p.join("README.md"), "# p\n").unwrap();
    }
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
    for name in ["probe.py", "claude"] {
        let path = bin.join(name);
        std::fs::write(&path, PROBE).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }
    let site = a.sites().join("probe");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(
        site.join("justfile"),
        format!("probe:\n    python3 {}/probe.py recipe\n", bin.display()),
    )
    .unwrap();
    std::fs::write(
        a.config_file(),
        format!(
            "[provider]\nbase_url = \"{base_url}\"\n\n[projects.\"{project}\"]\nexternal_harnesses = \
             [\"claude-code\"]\n\n[claude_code]\ncommand = \"{bin}/claude\"\n\n[sandbox]\nbinds_ro \
             = [\"{bin}\"]\nbinds_rw = [\"{out}\"]\n\n[mcp.probe]\ncommand = \"python3\"\nargs = \
             [\"{bin}/probe.py\", \"mcp\"]\n",
            project = project.display(),
            bin = bin.display(),
            out = out.display(),
        ),
    )
    .unwrap();
    ClaudeToken::new(TOKEN).unwrap().save(&home).unwrap();

    let host_tmp = tempfile::tempdir_in("/tmp").unwrap();
    std::fs::write(host_tmp.path().join("marker"), "host\n").unwrap();
    let user_home = PathBuf::from(std::env::var("HOME").unwrap());
    // What the user's $HOME holds that no spawn is given (toolchains, caches, `.gitconfig`), and
    // what only a Claude Code run is: its `.claude`, the package caches of `[claude_code]
    // read_dirs`, the path to the worker exe.
    let given = [
        ".cargo",
        ".rustup",
        ".bun",
        ".local",
        ".cache",
        ".config",
        ".gitconfig",
    ];
    let config = composer_server::Config::defaults();
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_composer"));
    let claude_only: Vec<String> = config
        .claude_code
        .read_dirs
        .iter()
        .map(|d| d.trim_start_matches("~/").to_string())
        .chain(
            exe.strip_prefix(&user_home)
                .ok()
                .map(|p| p.display().to_string()),
        )
        .chain([".claude".to_string()])
        .filter_map(|d| d.split('/').next().map(str::to_string))
        .collect();
    let entries: Vec<String> = std::fs::read_dir(&user_home)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !given.iter().any(|g| n.starts_with(g)))
        .take(40)
        .collect();
    let (home_entries_no_claude, home_entries): (Vec<String>, Vec<String>) =
        entries.into_iter().partition(|n| claude_only.contains(n));
    let unwritable = json!({
        "b's sites": b.sites().join("home"),
        "the library": home.library(),
        "the home's state": home.state(),
        "a's state": a.state(),
        "b's state": b.state(),
        "a's root": a.root(),
        "another project": theirs,
        "the main checkout": project,
    });
    std::fs::create_dir_all(b.state()).unwrap();
    std::fs::write(
        bin.join("paths.json"),
        json!({
            "hidden": {
                "b": b.root(),
                "b's page": b.sites().join("home/secret.md"),
                "the library": home.library(),
                "the home's state": home.state(),
                "claude-token": ClaudeToken::file(&home),
                "a's token": a.state().join("token"),
                "a's config": a.config_file(),
                "another project": theirs.join("README.md"),
                "~/.claude/.credentials.json": user_home.join(".claude/.credentials.json"),
                "~/.ssh": user_home.join(".ssh"),
                "~/.composer": user_home.join(".composer"),
            },
            "unwritable": unwritable,
            "home_entries": home_entries,
            "home_entries_no_claude": home_entries_no_claude,
            "sites": a.sites(),
            "a_state": a.state(),
            "home_templates": home.root().join("templates"),
            "ws_templates": a.root().join("templates"),
            "host_tmp": host_tmp.path().join("marker"),
            "ns": { "pid": ns("pid"), "ipc": ns("ipc"), "uts": ns("uts"), "cgroup": ns("cgroup") },
            "token": TOKEN,
            "worktree": a.worktrees().join("0001"),
            "project": project,
            "out": out,
        })
        .to_string(),
    )
    .unwrap();
    // SAFETY: set before any server or thread reads it; one test per binary.
    unsafe { std::env::set_var("COMPOSER_WORKER_EXE", env!("CARGO_BIN_EXE_composer")) };
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
    World {
        dir,
        home,
        a,
        b,
        project,
        out,
        unwritable: unwritable
            .as_object()
            .unwrap()
            .values()
            .map(|v| PathBuf::from(v.as_str().unwrap()))
            .collect(),
        server,
        _host_tmp: host_tmp,
    }
}

impl World {
    /// The probe as a site's recipe: its exit code and output, or the refusal.
    pub async fn recipe(&self) -> Result<(Option<i32>, String), String> {
        let justfile = Justfile::load(&self.a.sites().join("probe"))
            .await
            .map_err(|e| e.to_string())?;
        let env = RecipeEnv::new(&self.server, "probe", None);
        run_recipe(
            &justfile,
            "probe",
            &[],
            &env,
            Duration::from_secs(60),
            &CancellationToken::new(),
        )
        .await
        .map(|o| (o.exit_code, o.output))
    }

    /// The probe as an MCP server: its state and error in `mcp.list`.
    pub async fn mcp(&self) -> (String, Option<String>) {
        let listed = self.server.mcp().list(&[]).await;
        let probe = listed.into_iter().find(|s| s.name == "probe").unwrap();
        (probe.state, probe.error)
    }

    /// The probe as `claude` on ticket 0001: started, then the ticket file once it is done (or
    /// after 20s); or the refusal.
    pub async fn claude(&self) -> Result<String, String> {
        let orchestrator = self
            .server
            .sessions
            .create("orchestrator", Some(self.project.display().to_string()))
            .unwrap();
        let tickets = self.a.sites().join("orchestration/tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        let file = tickets.join("0001.md");
        std::fs::write(
            &file,
            format!(
                "---\ntitle: Ticket 0001\ntype: ticket\nid: \"0001\"\nstatus: open\nagent: worker\n\
                 project: {}\n---\n# Ticket 0001\n\n## Task\nProbe.\n\n## Result\n",
                self.project.display()
            ),
        )
        .unwrap();
        self.server
            .workers
            .spawn(
                &self.server,
                &orchestrator,
                &TicketId::parse("0001").unwrap(),
                None,
                Some("claude-code"),
            )
            .await?;
        let mut text = String::new();
        for _ in 0..200 {
            text = std::fs::read_to_string(&file).unwrap();
            if text.contains("status: done") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(text)
    }

    /// The probe's report as `kind`, if it ran.
    pub fn report(&self, kind: &str) -> Option<Value> {
        let text = std::fs::read_to_string(self.out.join(format!("{kind}.json"))).ok()?;
        Some(serde_json::from_str(&text).unwrap())
    }
}

impl World {
    /// The folders a spawn of `kind` wrote to that it may not.
    pub fn leaked_writes(&self, kind: &str) -> Vec<PathBuf> {
        self.unwritable
            .iter()
            .map(|f| f.join(format!("from-{kind}")))
            .filter(|f| f.exists())
            .collect()
    }
}

/// The checks of a report that failed.
pub fn failed(report: &Value) -> Vec<String> {
    report
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, ok)| *ok != &Value::Bool(true))
        .map(|(name, _)| name.clone())
        .collect()
}

impl World {
    /// Every run log under a's state, for a failure message.
    pub fn logs(&self) -> String {
        fn walk(dir: &Path, out: &mut String) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "log") {
                    out.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
                }
            }
        }
        let mut out = String::new();
        walk(&self.a.state(), &mut out);
        out
    }
}
