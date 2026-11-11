//! The app's `project/files?work=` route: the project's main folder without a work, the work's
//! `project/` view with one, and the same read/write rule as the agent's `project/`.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

use composer_protocol::http::AppOpened;
use composer_server::Config;
use composer_server::pipeline::{NewWork, new_work};
use composer_server::project::ProjectMode;
use support::start_with;

mod support;

fn git_init(dir: &std::path::Path) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "init",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
}

#[tokio::test]
async fn the_app_project_file_route_follows_the_work() {
    let projects = tempfile::tempdir().unwrap();
    let mut config = Config::defaults();
    config.projects.dir = projects.path().display().to_string();
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    // The server's startup pass links the works it finds; let it finish before this test's work.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let project = projects.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() { project }\n").unwrap();
    git_init(&project);

    let flow = dir.path().join("sites/flow");
    std::fs::create_dir_all(flow.join("stages")).unwrap();
    std::fs::create_dir_all(flow.join("ui/dist")).unwrap();
    std::fs::write(flow.join("ui/dist/index.html"), "<!doctype html>").unwrap();
    std::fs::write(
        flow.join("index.md"),
        "---\ntitle: Flow\ntype: pipeline\napp:\n  scope: project\n  when: [git]\n---\n# Flow\n",
    )
    .unwrap();
    std::fs::write(
        flow.join("stages/01_impl.md"),
        "---\ntitle: Implement\ntype: stage\nproject_mode: worktree\n---\n# 01 · Implement\n",
    )
    .unwrap();

    let work = new_work(
        &server,
        NewWork {
            pipeline: "flow",
            name: "save",
            title: None,
            project: Some(project.to_str().unwrap()),
            brief: Some("A save system."),
        },
        composer_protocol::ChangedBy::Agent,
    )
    .unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(worktree.join("main.rs"), "fn main() { worktree }\n").unwrap();
    work.project_mount(&server, &worktree, ProjectMode::Worktree);

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "http://{addr}/api/projects/project/apps/flow/open?token={}",
            server.token
        ))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    let opened: AppOpened = serde_json::from_str(&body)
        .unwrap_or_else(|e| panic!("opening the app: {status}: {body} ({e})"));
    let base = format!("http://{addr}{}", opened.url.split('?').next().unwrap());
    let token = opened
        .token
        .expect("an app opened on a project gets a token");
    let files = |path: &str, work: Option<&str>| {
        format!(
            "{base}api/project/files/{path}?app_token={token}{}",
            work.map(|w| format!("&work={w}")).unwrap_or_default()
        )
    };

    // Without a work: the project's main folder.
    let main = client.get(files("main.rs", None)).send().await.unwrap();
    assert!(main.status().is_success());
    assert_eq!(main.text().await.unwrap(), "fn main() { project }\n");

    // With the work: its project/ view, the worktree while the stage runs.
    let in_work = client
        .get(files("main.rs", Some("save")))
        .send()
        .await
        .unwrap();
    assert!(in_work.status().is_success());
    assert_eq!(in_work.text().await.unwrap(), "fn main() { worktree }\n");

    // A put follows the same view: the worktree, not the project.
    let put = client
        .put(files("new.rs", Some("save")))
        .body("fn added() {}\n")
        .send()
        .await
        .unwrap();
    assert!(put.status().is_success());
    assert!(worktree.join("new.rs").is_file());
    assert!(!project.join("new.rs").exists());

    // A read work refuses the put.
    work.project_mount(&server, &project, ProjectMode::Read);
    let refused = client
        .put(files("new.rs", Some("save")))
        .body("x")
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);

    // A work of another project is refused on this app's project.
    let elsewhere = projects.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("README.md"), "# elsewhere\n").unwrap();
    git_init(&elsewhere);
    new_work(
        &server,
        NewWork {
            pipeline: "flow",
            name: "other",
            title: None,
            project: Some(elsewhere.to_str().unwrap()),
            brief: None,
        },
        composer_protocol::ChangedBy::Agent,
    )
    .unwrap();
    let wrong = client
        .get(files("main.rs", Some("other")))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::FORBIDDEN);
}
