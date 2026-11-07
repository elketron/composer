//! Project apps with their own tokens (ticket 0163): an attached app is opened on a project with
//! the workspace token, which gives it a token of its own (in its URL once, then a cookie scoped
//! to its path). With it the app reads any page, writes only its own site and its project, runs
//! only its own site's actions, and its socket sends only what an app needs. Another project's or
//! another app's path, a forged or revoked token: not found. The workspace's cookie doesn't work
//! from an app's page.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use composer_server::Config;
use futures::{SinkExt, StreamExt};
use reqwest::{Client, Method, header};
use serde_json::{Value, json};
use support::*;
use tokio_tungstenite::tungstenite::Message;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

struct Call {
    addr: SocketAddr,
}

impl Call {
    /// `(status, body)` of `method path` with `auth` as a header (`Cookie: ...` or a bearer).
    async fn go(
        &self,
        method: Method,
        path: &str,
        auth: &[(header::HeaderName, String)],
        body: Option<Value>,
    ) -> (u16, String) {
        let mut request = Client::new().request(method, format!("http://{}{path}", self.addr));
        for (name, value) in auth {
            request = request.header(name, value);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        (
            response.status().as_u16(),
            response.text().await.unwrap_or_default(),
        )
    }
}

fn cookie(token: &str) -> Vec<(header::HeaderName, String)> {
    vec![(header::COOKIE, format!("composer_app={token}"))]
}

fn bearer(token: &str) -> Vec<(header::HeaderName, String)> {
    vec![(header::AUTHORIZATION, format!("Bearer {token}"))]
}

#[tokio::test]
async fn an_app_token_reads_the_web_and_writes_only_its_own() {
    let projects = tempfile::tempdir().unwrap();
    let root = projects.path().canonicalize().unwrap();
    let game = root.join("game");
    write(&game.join("parts/arm.FCStd"), "");
    let notes = root.join("notes");
    write(&notes.join("arm.FCStd"), "");
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), game.join("link")).unwrap();

    let mut config = Config::defaults();
    config.projects.dir = root.display().to_string();
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let sites = dir.path().join("sites");
    write(
        &sites.join("cad/index.md"),
        "---\ntitle: CAD\napp:\n  scope: project\n  when: [\"files:*.FCStd\"]\n---\n# CAD\n",
    );
    write(&sites.join("cad/ui/dist/index.html"), "<p>the cad app</p>");
    write(&sites.join("cad/justfile"), "hello:\n    echo hi\n");
    write(
        &sites.join("other/index.md"),
        "---\ntitle: Other\n---\n# Other\n",
    );
    write(&sites.join("other/justfile"), "hi:\n    echo hi\n");
    let call = Call { addr };
    let workspace = bearer(&server.token);

    // Opening needs the app attached to the project.
    let (status, body) = call
        .go(
            Method::POST,
            "/api/projects/game/apps/other/open",
            &workspace,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert!(
        body.contains("isn't one of project `game`'s apps"),
        "{body}"
    );
    let (status, body) = call
        .go(
            Method::POST,
            "/api/projects/game/apps/cad/open",
            &workspace,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    let token = opened["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("app_"));
    assert_eq!(opened["url"], format!("/p/game/cad/?app_token={token}"));

    // The URL loads the app and leaves the token in a cookie for the app's path only.
    let response = Client::new()
        .get(format!("http://{addr}/p/game/cad/?app_token={token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let set = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(set.contains(&format!("composer_app={token}")), "{set}");
    assert!(
        set.contains("Path=/p/game/cad/") && set.contains("HttpOnly"),
        "{set}"
    );
    assert!(response.text().await.unwrap().contains("the cad app"));
    let me = cookie(&token);

    // No token, a forged one, or the right one on another project or app: not found.
    for (path, auth) in [
        ("/p/game/cad/", vec![]),
        ("/p/game/cad/", bearer("app_forged")),
        ("/p/game/cad/", bearer(&server.token)),
        ("/p/notes/cad/", me.clone()),
        ("/p/game/other/", me.clone()),
    ] {
        let (status, _) = call.go(Method::GET, path, &auth, None).await;
        assert_eq!(status, 404, "{path} {auth:?}");
    }

    // Reads: any page.
    let (status, body) = call
        .go(
            Method::GET,
            "/p/game/cad/api/pages/other/index.md",
            &me,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    // Writes: its own site, not another.
    let page = json!({ "content": "---\ntitle: N\n---\nnote\n" });
    let (status, body) = call
        .go(
            Method::PUT,
            "/p/game/cad/api/pages/cad/notes.md",
            &me,
            Some(page.clone()),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = call
        .go(
            Method::PUT,
            "/p/game/cad/api/pages/other/x.md",
            &me,
            Some(page),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("can't write to site://other"), "{body}");
    assert!(!sites.join("other/x.md").exists());
    // Files: its own site's only.
    let (status, body) = call
        .go(
            Method::GET,
            "/p/game/cad/api/files/other/index.md",
            &me,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");

    // Its project: inside, not through a symlink out of it.
    let (status, body) = call
        .go(
            Method::PUT,
            "/p/game/cad/api/project/files/out/view.json",
            &me,
            Some(json!(1)),
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert!(game.join("out/view.json").is_file());
    let (status, body) = call
        .go(
            Method::GET,
            "/p/game/cad/api/project/files/parts/arm.FCStd",
            &me,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = call
        .go(
            Method::PUT,
            "/p/game/cad/api/project/files/link/x.txt",
            &me,
            Some(json!(1)),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(!outside.path().join("x.txt").exists());

    // Actions: its own site's (which still need the user's approval), not another's.
    let (status, body) = call
        .go(Method::POST, "/p/game/cad/api/actions/other/hi", &me, None)
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(
        body.contains("can't run the actions of site://other"),
        "{body}"
    );
    let (status, body) = call
        .go(Method::POST, "/p/game/cad/api/actions/cad/hello", &me, None)
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("needs the user's approval"), "{body}");

    // The workspace token still does everything; its cookie doesn't work from an app's page.
    let (status, _) = call
        .go(Method::GET, "/api/pages/other/index.md", &workspace, None)
        .await;
    assert_eq!(status, 200);
    let from_app = vec![
        (header::COOKIE, format!("composer_token={}", server.token)),
        (header::REFERER, format!("http://{addr}/p/game/cad/")),
    ];
    let (status, body) = call
        .go(Method::GET, "/api/pages/other/index.md", &from_app, None)
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("its own API"), "{body}");

    // Its socket: what an app needs, nothing else.
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/p/game/cad/ws?app_token={token}"))
            .await
            .unwrap();
    let ask = |id: u64, command: ClientCommand| {
        Message::Text(
            serde_json::to_string(&composer_protocol::ClientRequest { id, command })
                .unwrap()
                .into(),
        )
    };
    socket
        .send(ask(1, ClientCommand::SessionList))
        .await
        .unwrap();
    socket
        .send(ask(2, ClientCommand::AppsList { project: None }))
        .await
        .unwrap();
    let (mut refused, mut listed) = (None, None);
    while refused.is_none() || listed.is_none() {
        let next = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next()).await;
        let Ok(Some(Ok(Message::Text(text)))) = next else {
            panic!("no refusal and listing within 10s: refused {refused:?}, listed {listed:?}")
        };
        match serde_json::from_str::<ServerMessage>(&text).unwrap() {
            ServerMessage::Error {
                id: Some(1),
                message,
            } => refused = Some(message),
            ServerMessage::Ok {
                id: 2,
                result: CommandResult::Apps { list },
            } => listed = Some(list),
            _ => {}
        }
    }
    assert!(refused.unwrap().contains("can't send `session.list`"));
    assert_eq!(listed.unwrap().project.as_deref(), Some("game"));

    // Revoked: gone.
    let (status, body) = call
        .go(
            Method::DELETE,
            "/api/projects/game/apps/cad/tokens",
            &workspace,
            None,
        )
        .await;
    assert_eq!((status, body.as_str()), (200, "1"));
    let (status, _) = call.go(Method::GET, "/p/game/cad/", &me, None).await;
    assert_eq!(status, 404);
}
