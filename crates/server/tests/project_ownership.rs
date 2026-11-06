//! A project belongs to one workspace (ticket 0147): whichever workspace uses it first owns it;
//! the other can't use or see it under any spelling, nested folders included.

mod support;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use composer_server::{Home, Host, Server, WorkspaceName, init_home, init_workspace};
use support::MockProvider;

/// `<dir>/projects` with `x/sub`, `y` and `lnk → x`.
fn projects(dir: &Path) -> String {
    let projects = dir.join("projects");
    for d in ["x/sub", "y"] {
        std::fs::create_dir_all(projects.join(d)).unwrap();
    }
    std::os::unix::fs::symlink(projects.join("x"), projects.join("lnk")).unwrap();
    projects.display().to_string()
}

/// A host over `default` and `b`, both with `projects_dir`; `b_config` is written to b's own
/// config.toml first.
async fn host(home: &Home, projects_dir: &str, b_config: &str) -> composer_server::Result<Host> {
    let b = WorkspaceName::new("b").unwrap();
    init_workspace(&home.workspace(&b)).await.unwrap();
    std::fs::write(home.workspace(&b).config_file(), b_config).unwrap();
    let mut servers = BTreeMap::new();
    for name in home.workspace_names().unwrap() {
        let workspace = home.workspace(&name);
        let mut config = workspace.load_config().unwrap();
        config.projects.dir = projects_dir.to_string();
        let provider = Arc::new(MockProvider::new(vec![]));
        servers.insert(name, Server::new(workspace, config, provider).unwrap());
    }
    Host::new(home.clone(), servers)
}

fn server(host: &Host, name: &str) -> Arc<Server> {
    host.workspace(&WorkspaceName::new(name).unwrap()).unwrap()
}

#[tokio::test]
async fn a_project_used_by_one_workspace_is_closed_to_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = projects(dir.path());
    let home = Home::new(dir.path().join("home"));
    init_home(&home).await.unwrap();
    let host = host(&home, &projects_dir, "").await.unwrap();
    let (a, b) = (server(&host, "default"), server(&host, "b"));

    let x = a.use_project("x").unwrap();
    // Every spelling of x, and anything nested in it, is a's.
    let absolute = format!("{projects_dir}/x");
    for spelling in [
        "x",
        "x/sub",
        "lnk",
        "lnk/sub",
        "y/../x",
        "./x",
        absolute.as_str(),
    ] {
        let error = b.use_project(spelling).unwrap_err();
        assert!(error.contains("workspace `default`"), "{spelling}: {error}");
        assert!(b.see_project(spelling).is_err(), "{spelling}");
    }
    // a keeps working in it, nested folders included.
    assert_eq!(a.use_project("lnk").unwrap(), x);
    a.use_project("x/sub").unwrap();

    // b's own project is closed to a, and a folder containing a's project is refused to b.
    b.use_project("y").unwrap();
    assert!(a.use_project("y").is_err());
    assert!(a.see_project("y").is_err());
    let error = b.use_project(&projects_dir).unwrap_err();
    assert!(error.contains("through"), "{error}");

    // Listings leave out the other workspace's projects.
    let names = |s: &Server| -> Vec<String> {
        composer_server::project::project_list(s, "")
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    let (in_a, in_b) = (names(&a), names(&b));
    assert!(
        in_a.contains(&"x".to_string()) && !in_a.contains(&"y".to_string()),
        "{in_a:?}"
    );
    assert!(
        in_b.contains(&"y".to_string()) && !in_b.contains(&"x".to_string()),
        "{in_b:?}"
    );
}

#[tokio::test]
async fn ownership_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = projects(dir.path());
    let home = Home::new(dir.path().join("home"));
    init_home(&home).await.unwrap();
    {
        let host = host(&home, &projects_dir, "").await.unwrap();
        server(&host, "b").use_project("y").unwrap();
    }
    let host = host(&home, &projects_dir, "").await.unwrap();
    let error = server(&host, "default").use_project("y").unwrap_err();
    assert!(error.contains("workspace `b`"), "{error}");
}

#[tokio::test]
async fn two_configs_naming_one_project_stop_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = projects(dir.path());
    let home = Home::new(dir.path().join("home"));
    init_home(&home).await.unwrap();
    // default's config names x; b's names x/sub.
    let default = home.workspace(&WorkspaceName::default_workspace());
    std::fs::write(
        default.config_file(),
        format!("[projects.\"{projects_dir}/x\"]\nexternal_harnesses = []\n"),
    )
    .unwrap();
    let b_config = format!("[projects.\"{projects_dir}/x/sub\"]\nexternal_harnesses = []\n");
    let error = host(&home, &projects_dir, &b_config)
        .await
        .err()
        .expect("conflicting configs must not start")
        .to_string();
    // Whichever is claimed second is refused; the message names both workspaces.
    let names_both = (error.contains("workspace b's config.toml") && error.contains("`default`"))
        || (error.contains("workspace default's config.toml") && error.contains("`b`"));
    assert!(names_both, "{error}");
}

#[tokio::test]
async fn a_mount_may_not_point_into_another_workspaces_project() {
    let dir = tempfile::tempdir().unwrap();
    let projects_dir = projects(dir.path());
    std::fs::create_dir_all(format!("{projects_dir}/z")).unwrap();
    for d in ["x", "y", "z"] {
        std::fs::write(format!("{projects_dir}/{d}/note.md"), "# note\n").unwrap();
    }
    let home = Home::new(dir.path().join("home"));
    init_home(&home).await.unwrap();
    let host = host(&home, &projects_dir, "").await.unwrap();
    let (a, b) = (server(&host, "default"), server(&host, "b"));
    a.use_project("x").unwrap();
    b.use_project("y").unwrap();

    // Links in a's web to its own project, to b's, and to one nobody owns yet.
    let site = a.web.root().join("memory");
    for d in ["x", "y", "z"] {
        std::os::unix::fs::symlink(format!("{projects_dir}/{d}"), site.join(format!("mnt-{d}")))
            .unwrap();
    }
    let note = |d: &str| site.join(format!("mnt-{d}/note.md"));
    assert!(a.web.allows(&note("x")), "its own project");
    assert!(
        a.web.allows(&note("z")),
        "unowned: the projects directory allowance"
    );
    assert!(!a.web.allows(&note("y")), "b's project");
    assert!(a.web.check(&note("y")).is_err());
    // Once b claims z, a's mount to it closes too.
    b.use_project("z").unwrap();
    assert!(!a.web.allows(&note("z")));
}
