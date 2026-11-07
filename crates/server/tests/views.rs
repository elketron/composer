//! The HTML views (ticket 0183): a page rendered with no way to run what's in it, a project's
//! changes, both behind the workspace token.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_server::Config;
use support::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

async fn get(addr: SocketAddr, path: &str, token: Option<&str>) -> reqwest::Response {
    let mut request = reqwest::Client::new().get(format!("http://{addr}{path}"));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    request.send().await.unwrap()
}

#[tokio::test]
async fn a_page_renders_without_its_html_running() {
    let (dir, server, _provider, addr) = start(vec![]).await;
    write(
        &dir.path().join("sites/notes/evil.md"),
        "---\ntitle: Evil\n---\n# Evil\n\n<script>fetch('/api/sites')</script>\n\nsee [plan](site://plans/x.md)\n",
    );
    let token = server.token.clone();

    let refused = get(addr, "/view/notes/evil.md", None).await;
    assert!(
        !refused.status().is_success(),
        "no token: {}",
        refused.status()
    );

    let page = get(addr, "/view/notes/evil.md", Some(&token)).await;
    assert_eq!(page.status(), 200);
    let policy = page.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        policy.contains("default-src 'none'") && !policy.contains("script-src"),
        "{policy}"
    );
    let html = page.text().await.unwrap();
    assert!(!html.contains("<script"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "shown as text: {html}");
    assert!(html.contains("href=\"site://plans/x.md\""), "{html}");
    assert!(html.contains("<title>Evil</title>"), "{html}");

    assert_eq!(
        get(addr, "/view/notes/missing.md", Some(&token))
            .await
            .status(),
        404
    );
    // The site's index.
    write(
        &dir.path().join("sites/notes/index.md"),
        "---\ntitle: Notes\n---\n# Notes\n",
    );
    let index = get(addr, "/view/notes", Some(&token)).await;
    assert_eq!(index.status(), 200);
}

#[tokio::test]
async fn a_projects_changes_render() {
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    let game = root.join("game");
    write(&game.join("a.txt"), "one\n");
    git(&game, &["init", "-q", "-b", "main"]);
    git(&game, &["add", "."]);
    git(&game, &["commit", "-qm", "init"]);
    write(&game.join("a.txt"), "one\n<b>two</b>\n");
    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    let (_dir, server, _provider, addr) = start_with(config, vec![]).await;
    let token = server.token.clone();

    let changes = get(addr, "/view/changes/game", Some(&token)).await;
    assert_eq!(changes.status(), 200);
    let html = changes.text().await.unwrap();
    assert!(html.contains("a.txt"), "{html}");
    assert!(
        html.contains("<span class=\"add\">+&lt;b&gt;two&lt;/b&gt;</span>"),
        "{html}"
    );
    assert!(html.contains("main"), "the branch: {html}");

    assert_eq!(
        get(addr, "/view/changes/nope", Some(&token)).await.status(),
        404
    );
}
