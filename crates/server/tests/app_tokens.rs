//! Project apps with their own tokens (ticket 0163): an attached app is opened on a project with
//! the workspace token, which gives it a token of its own, and the app is served under it:
//! `/a/<token>/`, sandboxed (an opaque origin). With it the app reads any page, writes only its own
//! site and its project, runs only its own site's actions, and its socket sends only what an app
//! needs. A forged or revoked token: not found. The workspace's credentials don't work there, and
//! `Origin: null` (what the sandboxed page sends) works only there.
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

/// What the app's sandboxed page sends with every request.
fn null_origin() -> Vec<(header::HeaderName, String)> {
    vec![(header::ORIGIN, "null".to_string())]
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
    // Built with the current sdk (0169).
    write(
        &sites.join("cad/ui/dist/assets/a.js"),
        "d=\"composer-sdk:3\"",
    );
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
    assert_eq!(opened["url"], format!("/a/{token}/"));
    assert_eq!(opened["project"], "game");
    let at = |rest: &str| format!("/a/{token}/{rest}");

    // The URL loads the app, sandboxed, and sets no cookie: the token is the path. The page is
    // told its site, project and session.
    let response = Client::new()
        .get(format!("http://{addr}/a/{token}/"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()[header::CONTENT_SECURITY_POLICY],
        composer_server::api::SANDBOX
    );
    assert!(!composer_server::api::SANDBOX.contains("allow-same-origin"));
    assert!(response.headers().get(header::SET_COOKIE).is_none());
    let page = response.text().await.unwrap();
    assert!(page.contains("the cad app"), "{page}");
    for fact in [
        r#"<meta name="composer-app" content="{"#,
        "&quot;site&quot;:&quot;cad&quot;",
        "&quot;project&quot;:&quot;game&quot;",
        "&quot;session&quot;:null",
    ] {
        assert!(page.contains(fact), "{fact} in {page}");
    }
    // Its assets are sandboxed too (an HTML file there is no way out).
    let response = Client::new()
        .get(format!("http://{addr}/a/{token}/assets/a.js"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.headers()[header::CONTENT_SECURITY_POLICY],
        composer_server::api::SANDBOX
    );
    let me = null_origin();

    // No token, a forged one, the workspace's token or a session in its place: not found.
    let session = server
        .web_sessions
        .mint(composer_server::api::Credential::Local);
    for path in [
        "/a/".to_string(),
        "/a/app_forged/".to_string(),
        format!("/a/{}/", server.token),
        format!("/a/{session}/"),
    ] {
        let (status, _) = call.go(Method::GET, &path, &[], None).await;
        assert_eq!(status, 404, "{path}");
    }
    // The workspace's cookie or bearer doesn't open an app's routes either.
    for auth in [
        workspace.clone(),
        vec![(
            header::COOKIE,
            format!("composer_token_{}={session}", addr.port()),
        )],
    ] {
        let (status, _) = call
            .go(
                Method::GET,
                "/a/app_forged/api/pages/other/index.md",
                &auth,
                None,
            )
            .await;
        assert_eq!(status, 404, "{auth:?}");
    }
    // Another site's origin: refused, even with the token.
    let (status, _) = call
        .go(
            Method::GET,
            &at("api/sites"),
            &[(header::ORIGIN, "http://evil.example".into())],
            None,
        )
        .await;
    assert_eq!(status, 403);

    // The old URLs: to the new one, only with that app's token there; nothing else is served.
    let response = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!(
            "http://{addr}/p/game/cad/?app_token={token}&session=s1"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    assert_eq!(
        response.headers()[header::LOCATION],
        format!("/a/{token}/?session=s1").as_str()
    );
    for path in [
        format!("/p/notes/cad/?app_token={token}"),
        format!("/p/game/other/?app_token={token}"),
        format!("/h/cad/?app_token={token}"),
        "/p/game/cad/".to_string(),
    ] {
        let (status, _) = call.go(Method::GET, &path, &[], None).await;
        assert_eq!(status, 404, "{path}");
    }

    // The sandboxed page's requests (`Origin: null`) are answered, readable by it (CORS), and
    // its preflight too.
    let response = Client::new()
        .get(format!("http://{addr}/a/{token}/api/pages/other/index.md"))
        .header(header::ORIGIN, "null")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "null"
    );
    let response = Client::new()
        .request(
            Method::OPTIONS,
            format!("http://{addr}/a/{token}/api/pages/cad/notes.md"),
        )
        .header(header::ORIGIN, "null")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PUT")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    assert_eq!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "null"
    );

    // Reads: any page.
    let (status, body) = call
        .go(Method::GET, &at("api/pages/other/index.md"), &me, None)
        .await;
    assert_eq!(status, 200, "{body}");
    // Writes: its own site, not another.
    let page = json!({ "content": "---\ntitle: N\n---\nnote\n" });
    let (status, body) = call
        .go(
            Method::PUT,
            &at("api/pages/cad/notes.md"),
            &me,
            Some(page.clone()),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = call
        .go(Method::PUT, &at("api/pages/other/x.md"), &me, Some(page))
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("can't write to site://other"), "{body}");
    assert!(!sites.join("other/x.md").exists());
    // Files: its own site's only.
    let (status, body) = call
        .go(Method::GET, &at("api/files/other/index.md"), &me, None)
        .await;
    assert_eq!(status, 403, "{body}");

    // Its project: inside, not through a symlink out of it.
    let (status, body) = call
        .go(
            Method::PUT,
            &at("api/project/files/out/view.json"),
            &me,
            Some(json!(1)),
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert!(game.join("out/view.json").is_file());
    let (status, body) = call
        .go(
            Method::GET,
            &at("api/project/files/parts/arm.FCStd"),
            &me,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = call
        .go(
            Method::PUT,
            &at("api/project/files/link/x.txt"),
            &me,
            Some(json!(1)),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(!outside.path().join("x.txt").exists());

    // Actions: its own site's (which still need the user's approval), not another's.
    let (status, body) = call
        .go(Method::POST, &at("api/actions/other/hi"), &me, None)
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(
        body.contains("can't run the actions of site://other"),
        "{body}"
    );
    let (status, body) = call
        .go(Method::POST, &at("api/actions/cad/hello"), &me, None)
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("needs the user's approval"), "{body}");

    // The workspace token still does everything; its cookie doesn't work from an app's page.
    let (status, _) = call
        .go(Method::GET, "/api/pages/other/index.md", &workspace, None)
        .await;
    assert_eq!(status, 200);
    let from_app = vec![
        (
            header::COOKIE,
            format!("composer_token_{}={}", addr.port(), server.token),
        ),
        (header::REFERER, format!("http://{addr}/a/{token}/")),
    ];
    let (status, body) = call
        .go(Method::GET, "/api/pages/other/index.md", &from_app, None)
        .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("its own API"), "{body}");

    // Its socket, from the sandboxed page (`Origin: null`): what an app needs, nothing else.
    let mut handshake =
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(format!(
            "ws://{addr}/a/{token}/ws"
        ))
        .unwrap();
    handshake
        .headers_mut()
        .insert(header::ORIGIN, "null".parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(handshake).await.unwrap();
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
    let (status, _) = call.go(Method::GET, &at(""), &me, None).await;
    assert_eq!(status, 404);
}
