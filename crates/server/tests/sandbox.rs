//! The sandbox (ticket 0157), with the real `bwrap`: a spawn of workspace a sees a's sites, its
//! project and its own `/tmp`; nothing of workspace b, the library, any `state/`, the rest of
//! `$HOME`, or the host's `/tmp`. Binds hide other workspaces' projects and never reach into the
//! composer home. Without a working `bwrap`, nothing runs.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use composer_server::actions::{Justfile, RecipeEnv, run_recipe};
use composer_server::sandbox::SandboxSpec;
use composer_server::{
    Home, SandboxConfig, Server, WorkspaceHome, WorkspaceName, init_home, init_workspace,
};
use tokio_util::sync::CancellationToken;

struct Fixture {
    dir: tempfile::TempDir,
    home: Home,
    a: WorkspaceHome,
    b: WorkspaceHome,
    /// a's project, and one b owns, side by side in `projects/`.
    mine: PathBuf,
    theirs: PathBuf,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path().join("composer"));
    init_home(&home).await.unwrap();
    let b = home.workspace(&WorkspaceName::new("b").unwrap());
    init_workspace(&b).await.unwrap();
    let a = home.workspace(&WorkspaceName::default_workspace());
    let mine = dir.path().join("projects/mine");
    let theirs = dir.path().join("projects/theirs");
    for project in [&mine, &theirs] {
        std::fs::create_dir_all(project).unwrap();
        std::fs::write(project.join("README"), "a project\n").unwrap();
    }
    std::fs::write(b.sites().join("home/secret.md"), "b's secret\n").unwrap();
    Fixture {
        dir,
        home,
        a,
        b,
        mine,
        theirs,
    }
}

fn spec(f: &Fixture, config: SandboxConfig) -> SandboxSpec {
    SandboxSpec::new(&f.a, config, vec![f.theirs.clone()])
}

fn config() -> SandboxConfig {
    composer_server::Config::defaults().sandbox
}

/// Runs `script` with `sh` in `spec`'s world: (exit code, output).
async fn run(spec: &SandboxSpec, script: &str) -> (Option<i32>, String) {
    let output = spec
        .command("sh")
        .unwrap()
        .arg("-c")
        .arg(script)
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    (output.status.code(), text)
}

/// A shell line failing with `name` when `path` exists.
fn absent(name: &str, path: &Path) -> String {
    format!(
        "if [ -e '{}' ]; then echo \"LEAK {name}\"; exit 1; fi\n",
        path.display()
    )
}

#[tokio::test]
async fn a_spawn_sees_its_workspace_and_project_and_nothing_else() {
    let f = fixture().await;
    let host_tmp = tempfile::tempdir_in("/tmp").unwrap();
    std::fs::write(host_tmp.path().join("marker"), "host\n").unwrap();
    let user_home = PathBuf::from(std::env::var("HOME").unwrap());
    let private_name = format!(
        "{}-private",
        host_tmp.path().file_name().unwrap().to_string_lossy()
    );
    let world = spec(&f, config()).with_rw(&f.mine).in_dir(f.a.sites());
    let mut script = String::from("set -e\n");
    for (name, path) in [
        ("b", f.b.root().to_path_buf()),
        ("b's page", f.b.sites().join("home/secret.md")),
        ("the library", f.home.library().join("home")),
        ("the home's state", f.home.state()),
        ("a's state", f.a.state()),
        ("a's config", f.a.config_file()),
        ("b's project", f.theirs.join("README")),
        ("~/.ssh", user_home.join(".ssh")),
        ("~/.composer", user_home.join(".composer")),
        ("the host's /tmp", host_tmp.path().join("marker")),
    ] {
        script += &absent(name, &path);
    }
    script += &format!(
        "test \"$PWD\" = '{sites}'\n\
         echo from-sandbox > '{sites}/home/written.md'\n\
         echo from-sandbox > '{mine}/written'\n\
         echo private > '/tmp/{private}'\n\
         test -r '{global}/config.toml'\n\
         test -d '{root}/templates/sdk'\n\
         if {{ echo x > '{global}/x'; }} 2>/dev/null; then echo 'LEAK global writable'; exit 1; fi\n\
         if {{ echo x > /usr/x; }} 2>/dev/null; then echo 'LEAK /usr writable'; exit 1; fi\n\
         echo ok\n",
        sites = f.a.sites().display(),
        mine = f.mine.display(),
        global = f.home.global().display(),
        root = f.a.root().display(),
        private = private_name,
    );
    let (code, out) = run(&world, &script).await;
    assert_eq!(code, Some(0), "{out}");
    assert!(out.trim_end().ends_with("ok"), "{out}");
    assert!(f.a.sites().join("home/written.md").is_file());
    assert!(f.mine.join("written").is_file());
    assert!(
        !Path::new("/tmp").join(&private_name).exists(),
        "/tmp isn't private"
    );
}

#[tokio::test]
async fn binds_hide_other_workspaces_projects_and_never_reach_the_home() {
    let f = fixture().await;
    let projects = f.dir.path().join("projects");
    let mut wide = config();
    wide.binds_rw = vec![projects.display().to_string()];
    let world = spec(&f, wide);
    let script = format!(
        "test -r '{}/README' && {}echo ok",
        f.mine.display(),
        absent("b's project", &f.theirs.join("README"))
    );
    let (code, out) = run(&world, &script).await;
    assert_eq!(code, Some(0), "{out}");

    for (bind, why) in [
        (
            f.home.root().to_path_buf(),
            "holds part of the composer home",
        ),
        (
            f.dir.path().to_path_buf(),
            "holds part of the composer home",
        ),
        (f.a.root().to_path_buf(), "holds part of the composer home"),
        (f.b.sites(), "is part of the composer home"),
        (f.home.library(), "is part of the composer home"),
        (f.a.state(), "is part of the composer home"),
        (f.home.state().join("x"), "is part of the composer home"),
        (f.theirs.join("src"), "a project of another workspace"),
    ] {
        let mut config = config();
        config.binds_ro = vec![bind.display().to_string()];
        let error = spec(&f, config).command("true").unwrap_err();
        assert!(error.contains(why), "{}: {error}", bind.display());
    }
    // A ticket worktree of a's own is fine.
    let worktree = f.a.worktrees().join("0001");
    std::fs::create_dir_all(&worktree).unwrap();
    assert!(
        spec(&f, config())
            .with_rw(&worktree)
            .command("true")
            .is_ok()
    );
}

#[tokio::test]
async fn a_recipe_runs_in_its_workspace_sandbox() {
    let f = fixture().await;
    let site = f.a.sites().join("probe");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(
        site.join("justfile"),
        format!(
            "probe:\n    test ! -e '{}'\n    test -d \"$COMPOSER_SDK\"\n    echo ran > out.txt\n",
            f.b.root().display()
        ),
    )
    .unwrap();
    let server = Server::new(
        f.a.clone(),
        f.a.load_config().unwrap(),
        Arc::new(support::MockProvider::new(vec![])),
    )
    .unwrap();
    let justfile = Justfile::load(&site).await.unwrap();
    let env = RecipeEnv::new(&server, "probe", None);
    let output = run_recipe(
        &justfile,
        "probe",
        &[],
        &env,
        Duration::from_secs(30),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(output.exit_code, Some(0), "{}", output.output);
    assert!(site.join("out.txt").is_file());
}

#[tokio::test]
async fn without_a_working_bwrap_nothing_runs() {
    let f = fixture().await;
    let missing = spec(&f, config()).bwrap_at("/nonexistent/bwrap");
    let error = missing.command("true").unwrap_err();
    assert!(error.contains("install bubblewrap"), "{error}");
    // One that can't build a namespace.
    let broken = f.dir.path().join("broken-bwrap");
    std::fs::write(
        &broken,
        "#!/bin/sh\necho 'no user namespaces' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&broken, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let error = spec(&f, config())
        .bwrap_at(&broken)
        .command("true")
        .unwrap_err();
    assert!(error.contains("no user namespaces"), "{error}");
}

#[tokio::test]
async fn turned_off_it_runs_as_is() {
    let f = fixture().await;
    let mut off = config();
    off.enabled = false;
    let (code, out) = run(
        &spec(&f, off),
        &format!("test -e '{}' && echo unsandboxed", f.b.root().display()),
    )
    .await;
    assert_eq!(code, Some(0), "{out}");
}

#[tokio::test]
async fn cargo_builds_inside() {
    let f = fixture().await;
    let krate = f.mine.join("hello");
    std::fs::create_dir_all(krate.join("src")).unwrap();
    std::fs::write(
        krate.join("Cargo.toml"),
        "[package]\nname = \"hello\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        krate.join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    let world = spec(&f, config()).with_rw(&f.mine).in_dir(&krate);
    let (code, out) = run(&world, "cargo run --offline -q").await;
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("hi"), "{out}");
}
