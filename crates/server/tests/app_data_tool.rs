//! Agents read app data (ticket 0171): `open` of `data://<project>/<app>/<path>` reads what an app
//! keeps for a project (text as text, an image shown, a folder listed), `data://<project>/` lists
//! the apps with data, `data://@home/<app>/...` an app's Home data. Read-only, inside the app's
//! folder, only this workspace's projects, and only the apps of sites the agent may read.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::path::Path;
use std::sync::Arc;

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_server::Config;
use composer_server::agent::AgentDefinition;
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A 1×1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

#[tokio::test]
async fn an_agent_sees_what_an_app_keeps() {
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("game")).unwrap();
    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    let (dir, server, _provider, _addr) = start_with(config, vec![]).await;
    for (name, sites) in [("all", "all"), ("cadonly", "[cad]")] {
        server
            .write_page(
                &SiteUrl::new("agents", &format!("{name}.md")).unwrap(),
                &format!(
                    "---\ntitle: {name}\ntype: agent\nname: {name}\ntools: [open]\nsites: {sites}\n---\nReads.\n"
                ),
                ChangedBy::External,
            )
            .unwrap();
    }
    let state = dir.path().join("state");
    let cad = state.join("projects/game/cad");
    write(&cad.join("views/arm.json"), br#"{"zoom": 2}"#);
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap();
    write(&cad.join("render.png"), &png);
    write(&state.join("projects/game/video/cut.json"), b"[1,2]");
    write(&state.join("home/calendar/today.md"), b"# 3 due\n");
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("secret.txt"), b"SECRET");
    std::os::unix::fs::symlink(outside.path(), cad.join("link")).unwrap();

    let session = server.sessions.create("all", None).unwrap();
    let open = |agent: &str, url: &str| {
        let context = ToolContext {
            server: server.clone(),
            session: server.sessions.get(session.id()).unwrap(),
            agent: Arc::new(AgentDefinition::load(&server.web, agent).unwrap()),
            cancel: CancellationToken::new(),
        };
        let (server, url) = (server.clone(), url.to_string());
        async move {
            server
                .tools
                .call_value(&context, "open", json!({ "url": url }))
                .await
        }
    };

    let read = open("all", "data://game/cad/views/arm.json").await;
    assert!(
        !read.is_error && read.text.contains(r#"{"zoom": 2}"#),
        "{}",
        read.text
    );
    let shown = open("all", "data://game/cad/render.png").await;
    assert!(!shown.is_error && shown.images.len() == 1, "{}", shown.text);
    let listed = open("all", "data://game/cad/").await;
    assert!(
        listed.text.contains("views/arm.json") && listed.text.contains("render.png"),
        "{}",
        listed.text
    );
    let apps = open("all", "data://game/").await;
    assert!(
        apps.text.contains("data://game/cad/") && apps.text.contains("data://game/video/"),
        "{}",
        apps.text
    );
    let home = open("all", "data://@home/calendar/today.md").await;
    assert!(home.text.contains("3 due"), "{}", home.text);

    // Not out of the app's folder, not another project, not an app the agent can't read.
    for (agent, url, says) in [
        ("all", "data://game/cad/../video/cut.json", "isn't inside"),
        ("all", "data://game/cad/link/secret.txt", "isn't inside"),
        (
            "all",
            "data://elsewhere/cad/x.json",
            "no project `elsewhere`",
        ),
        (
            "cadonly",
            "data://game/video/cut.json",
            "may not read site `video`",
        ),
    ] {
        let refused = open(agent, url).await;
        assert!(
            refused.is_error && refused.text.contains(says) && !refused.text.contains("SECRET"),
            "{agent} {url}: {}",
            refused.text
        );
    }
    let limited = open("cadonly", "data://game/").await;
    assert!(
        limited.text.contains("data://game/cad/") && !limited.text.contains("video"),
        "{}",
        limited.text
    );
}
