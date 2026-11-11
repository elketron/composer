//! Home apps open with a token of their own (tickets 0168, 0170): `POST /api/apps/<site>/open` (or ws
//! `app.open`) gives `<base>/a/<token>/`; with it the app writes only its own site and
//! its Home data, has no project. The workspace's own route (`<base>/<site>/`) serves only client
//! apps (the user's `[apps] clients`, the chat app), opened with a one-time code that becomes the
//! cookie: the workspace token is never in an app's URL. A site's own `client: true` only asks:
//! unlisted, it opens with a token like any app. A project-only app doesn't open on Home.
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use reqwest::{Client, Method, header};
use serde_json::{Value, json};
use support::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

async fn call(
    addr: SocketAddr,
    method: Method,
    path: &str,
    auth: Option<&str>,
    body: Option<Value>,
) -> reqwest::Response {
    let mut request = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .request(method, format!("http://{addr}{path}"));
    if let Some(token) = auth {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.unwrap()
}

async fn status_text(response: reqwest::Response) -> (u16, String) {
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

#[tokio::test]
async fn home_apps_have_their_own_tokens_and_clients_a_one_time_code() {
    let mut config = composer_server::Config::defaults();
    config.apps.clients = vec!["talk".into()];
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let sites = dir.path().join("sites");
    write(&sites.join("cal/index.md"), "---\ntitle: Cal\n---\n# Cal\n");
    write(&sites.join("cal/ui/dist/index.html"), "<p>cal</p>");
    write(
        &sites.join("talk/index.md"),
        "---\ntitle: Talk\napp:\n  client: true\n---\n# Talk\n",
    );
    write(&sites.join("talk/ui/dist/index.html"), "<p>talk</p>");
    // A client app is served only as the user approved its build.
    server
        .client_apps
        .approve("talk", &sites.join("talk/ui/dist"))
        .unwrap();
    // Asks to be a client; not listed.
    write(
        &sites.join("asker/index.md"),
        "---\ntitle: Asker\napp:\n  client: true\n---\n# Asker\n",
    );
    write(&sites.join("asker/ui/dist/index.html"), "<p>asker</p>");
    write(
        &sites.join("parts/index.md"),
        "---\ntitle: Parts\napp:\n  scope: project\n---\n# Parts\n",
    );
    let workspace = server.token.clone();

    // A plain Home app: its own token, served under it.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/cal/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    let token = opened["token"].as_str().unwrap().to_string();
    assert_eq!(opened["url"], format!("/a/{token}/"));
    assert!(opened.get("project").is_none(), "{opened}");
    let page = call(addr, Method::GET, &format!("/a/{token}/"), None, None).await;
    assert!(page.headers().get(header::SET_COOKIE).is_none());
    assert_eq!(
        page.headers()[header::CONTENT_SECURITY_POLICY],
        composer_server::api::SANDBOX
    );
    assert!(page.text().await.unwrap().contains("cal"));
    let me = Some(token.as_str());
    let note = json!({ "content": "---\ntitle: N\n---\nn\n" });
    let (status, _) = status_text(
        call(
            addr,
            Method::PUT,
            &format!("/a/{token}/api/pages/cal/n.md"),
            me,
            Some(note.clone()),
        )
        .await,
    )
    .await;
    assert_eq!(status, 200);
    let (status, body) = status_text(
        call(
            addr,
            Method::PUT,
            &format!("/a/{token}/api/pages/talk/n.md"),
            me,
            Some(note),
        )
        .await,
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("opened on Home"), "{body}");
    let (status, body) = status_text(
        call(
            addr,
            Method::GET,
            &format!("/a/{token}/api/project/files/x"),
            me,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 403, "{body}");
    let (_, body) = status_text(
        call(
            addr,
            Method::GET,
            &format!("/a/{token}/api/project"),
            me,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(body, "null");
    let (status, _) = status_text(
        call(
            addr,
            Method::PUT,
            &format!("/a/{token}/api/data/view.json"),
            me,
            Some(json!(1)),
        )
        .await,
    )
    .await;
    assert_eq!(status, 204);
    assert!(dir.path().join("state/home/cal/view.json").is_file());

    // The workspace's own route doesn't serve it.
    let (status, body) =
        status_text(call(addr, Method::GET, "/cal/", Some(&workspace), None).await).await;
    assert_eq!(status, 404);
    assert!(body.contains("a token of its own"), "{body}");

    // A client app: a one-time code, not the workspace token, in its URL.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/talk/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    let url = opened["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("/talk/?once="), "{url}");
    assert!(!url.contains(&workspace) && opened.get("token").is_none());
    let page = call(addr, Method::GET, &url, None, None).await;
    assert_eq!(page.status(), 200);
    let cookie = page.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    // The cookie is a browser session, named for this listener's port (the server is served
    // alone here): never the workspace token.
    assert!(
        cookie.starts_with(&format!("composer_token_{}=ses_", addr.port())),
        "{cookie}"
    );
    assert!(!cookie.contains(&workspace), "{cookie}");
    // Once.
    assert_eq!(
        call(addr, Method::GET, &url, None, None).await.status(),
        404
    );

    // Asking isn't being: a token of its own, and the listing says it asks.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/asker/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    assert!(
        opened["url"].as_str().unwrap().starts_with("/a/app_"),
        "{opened}"
    );
    let (status, _) =
        status_text(call(addr, Method::GET, "/asker/", Some(&workspace), None).await).await;
    assert_eq!(status, 404, "not served on the workspace's route");
    let (_, body) =
        status_text(call(addr, Method::GET, "/api/apps", Some(&workspace), None).await).await;
    let home: Value = serde_json::from_str(&body).unwrap();
    let errors = home["errors"].to_string();
    assert!(
        errors.contains("site://asker asks to be a client app"),
        "{errors}"
    );
    let client = |site: &str| {
        home["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["site"] == site)
            .unwrap()["client"]
            .as_bool()
            .unwrap()
    };
    assert!(client("talk") && !client("asker"));

    // A project app doesn't open on Home.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/parts/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 404);
    assert!(body.contains("open it from a project"), "{body}");

    // ws `app.open`.
    let mut socket = connect(addr, &workspace).await;
    send(
        &mut socket,
        1,
        ClientCommand::AppOpen {
            site: "cal".into(),
            project: None,
            session: None,
        },
    )
    .await;
    let events = until(&mut socket, |e| {
        matches!(
            e,
            ServerMessage::Ok { id: 1, .. } | ServerMessage::Error { id: Some(1), .. }
        )
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::AppOpened { opened },
        ..
    }) = events.last()
    else {
        panic!("{events:?}")
    };
    assert!(opened.url.starts_with("/a/app_"));
}

/// The `name=value` of the auth cookie a response sets.
fn session_cookie(response: &reqwest::Response) -> String {
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

async fn with_cookie(addr: SocketAddr, path: &str, cookie: &str) -> u16 {
    Client::new()
        .get(format!("http://{addr}{path}"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn a_one_time_code_opens_a_session_of_whoever_asked_for_it() {
    use composer_server::api::Credential;
    let mut config = composer_server::Config::defaults();
    config.apps.clients = vec!["talk".into()];
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let sites = dir.path().join("sites");
    write(&sites.join("talk/index.md"), "---\ntitle: Talk\n---\n");
    write(&sites.join("talk/ui/dist/index.html"), "<p>talk</p>");
    // A client app is served only as the user approved its build.
    server
        .client_apps
        .approve("talk", &sites.join("talk/ui/dist"))
        .unwrap();
    let local = server.token.clone();
    let code = server.devices.create_code("phone").unwrap();
    let (_, device) = server.devices.redeem(&code, None).unwrap().unwrap();
    let name = format!("composer_token_{}", addr.port());

    // Opens the client app with `token` over HTTP; the cookie its one-time code turns into.
    let open = |token: String| async move {
        let (status, body) = status_text(
            call(
                addr,
                Method::POST,
                "/api/apps/talk/open",
                Some(&token),
                None,
            )
            .await,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        let opened: Value = serde_json::from_str(&body).unwrap();
        let page = call(
            addr,
            Method::GET,
            opened["url"].as_str().unwrap(),
            None,
            None,
        )
        .await;
        assert_eq!(page.status(), 200);
        session_cookie(&page)
    };

    // A paired device asks: the session is that device's, not the local user's, and holds
    // neither token.
    let cookie = open(device.clone()).await;
    let (cookie_name, session) = cookie.split_once('=').unwrap();
    assert_eq!(cookie_name, name);
    assert!(session.starts_with("ses_"), "{cookie}");
    assert!(!cookie.contains(&local) && !cookie.contains(&device));
    let kind = server.credential(session).unwrap();
    assert!(
        matches!(&kind, Credential::Device { name, .. } if name == "phone"),
        "{kind:?}"
    );
    assert!(!kind.may_drive());
    assert_eq!(with_cookie(addr, "/api/info", &cookie).await, 200);
    // The plain cookie name belongs to no server served alone, and the session is no token of
    // another listener.
    assert_eq!(
        with_cookie(addr, "/api/info", &format!("composer_token={session}")).await,
        404
    );
    assert_eq!(
        with_cookie(addr, "/api/info", &format!("composer_token_1={session}")).await,
        404
    );
    // The local token is never accepted as a cookie's session by mistake of kind.
    assert_eq!(server.credential(&local), Some(Credential::Local));

    // Over the socket (`app.open`) the code stands for the socket's credential too.
    let mut socket = connect(addr, &device).await;
    send(
        &mut socket,
        1,
        ClientCommand::AppOpen {
            site: "talk".into(),
            project: None,
            session: None,
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(
            e,
            ServerMessage::Ok { id: 1, .. } | ServerMessage::Error { .. }
        )
    })
    .await;
    let Some(ServerMessage::Ok {
        result: CommandResult::AppOpened { opened },
        ..
    }) = reply.last()
    else {
        panic!("{:?}", reply.last())
    };
    let page = call(addr, Method::GET, &opened.url, None, None).await;
    let by_socket = session_cookie(&page);
    let by_socket = by_socket.split_once('=').unwrap().1;
    assert!(matches!(
        server.credential(by_socket),
        Some(Credential::Device { .. })
    ));

    // The local user asks: a local session.
    let mine = open(local.clone()).await;
    let mine_session = mine.split_once('=').unwrap().1.to_string();
    assert!(!mine.contains(&local));
    assert_eq!(server.credential(&mine_session), Some(Credential::Local));

    // `?token=` leaves a session in the cookie jar as well, of the token's kind.
    for (token, local_kind) in [(&local, true), (&device, false)] {
        let page = call(
            addr,
            Method::GET,
            &format!("/api/info?token={token}"),
            None,
            None,
        )
        .await;
        assert_eq!(page.status(), 200);
        let cookie = session_cookie(&page);
        assert!(cookie.starts_with(&format!("{name}=ses_")), "{cookie}");
        assert!(!cookie.contains(token.as_str()));
        let kind = server
            .credential(cookie.split_once('=').unwrap().1)
            .unwrap();
        assert_eq!(kind == Credential::Local, local_kind, "{kind:?}");
        // A request that already holds a working cookie gets no new one.
        let again = Client::new()
            .get(format!("http://{addr}/api/info?token={token}"))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert!(again.headers().get(header::SET_COOKIE).is_none());
    }

    // Revoking the device ends its sessions at once; the local one lives on.
    assert!(server.devices.revoke("phone").unwrap());
    assert_eq!(with_cookie(addr, "/api/info", &cookie).await, 404);
    assert_eq!(server.credential(session), None);
    assert_eq!(server.credential(by_socket), None);
    assert_eq!(with_cookie(addr, "/api/info", &mine).await, 200);

    // A code a device asked for is worth nothing once the device is revoked.
    let code = server.devices.create_code("tablet").unwrap();
    let (_, tablet) = server.devices.redeem(&code, None).unwrap().unwrap();
    let (_, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/talk/open",
            Some(&tablet),
            None,
        )
        .await,
    )
    .await;
    let opened: Value = serde_json::from_str(&body).unwrap();
    assert!(server.devices.revoke("tablet").unwrap());
    let page = call(
        addr,
        Method::GET,
        opened["url"].as_str().unwrap(),
        None,
        None,
    )
    .await;
    assert_eq!(page.status(), 404);
}

#[tokio::test]
async fn the_workspaces_route_serves_no_workflow_and_no_broken_manifest() {
    let mut config = composer_server::Config::defaults();
    config.apps.clients = vec!["talk".into(), "broken".into(), "listed".into()];
    let (dir, server, _provider, addr) = start_with(config, vec![]).await;
    let sites = dir.path().join("sites");
    // A workflow (`app:` names its application) that also has a built ui.
    write(
        &sites.join("flow/index.md"),
        "---\ntitle: Flow\napp: blender\nmcp: [blender]\n---\n# Flow\n",
    );
    write(&sites.join("flow/ui/dist/index.html"), "<p>flow app</p>");
    // Built with the current sdk (0169).
    write(
        &sites.join("flow/ui/dist/assets/a.js"),
        "d=\"composer-sdk:3\"",
    );
    // Manifests that do not parse; one of them is even listed as a client.
    write(
        &sites.join("broken/index.md"),
        "---\ntitle: Broken\napp:\n  colour: red\n---\n",
    );
    write(
        &sites.join("broken/ui/dist/index.html"),
        "<p>broken app</p>",
    );
    write(
        &sites.join("list/index.md"),
        "---\ntitle: List\napp: [blender]\n---\n",
    );
    write(&sites.join("list/ui/dist/index.html"), "<p>list app</p>");
    // A listed client whose manifest is a workflow string: a client app like any other.
    write(
        &sites.join("listed/index.md"),
        "---\ntitle: Listed\napp: blender\n---\n",
    );
    write(
        &sites.join("listed/ui/dist/index.html"),
        "<p>listed app</p>",
    );
    let workspace = server.token.clone();

    for (path, says, never) in [
        ("/flow/", "a token of its own", "flow app"),
        ("/flow/index.html", "a token of its own", "flow app"),
        ("/broken/", "is not served", "broken app"),
        ("/broken/index.html", "has no `colour`", "broken app"),
        ("/list/", "is not served", "list app"),
        ("/nosuchsite/", "is not served", "app</p>"),
    ] {
        let (status, body) =
            status_text(call(addr, Method::GET, path, Some(&workspace), None).await).await;
        assert_eq!(status, 404, "{path}: {body}");
        assert!(body.contains(says), "{path}: {body}");
        assert!(!body.contains(never), "{path}: {body}");
    }
    server
        .client_apps
        .approve("listed", &sites.join("listed/ui/dist"))
        .unwrap();
    let (status, body) =
        status_text(call(addr, Method::GET, "/listed/", Some(&workspace), None).await).await;
    assert_eq!((status, body.as_str()), (200, "<p>listed app</p>"));

    // The workflow's ui is an app like one without `app:`: opened on Home with its own token.
    let (status, body) = status_text(
        call(
            addr,
            Method::POST,
            "/api/apps/flow/open",
            Some(&workspace),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let opened: Value = serde_json::from_str(&body).unwrap();
    let url = opened["url"].as_str().unwrap();
    assert!(url.starts_with("/a/app_"), "{url}");
    let (status, body) = status_text(call(addr, Method::GET, url, None, None).await).await;
    assert_eq!(status, 200, "{body}");
    // The page is told where it is (`<meta name="composer-app">`), and is otherwise as built.
    assert!(
        body.starts_with(r#"<meta name="composer-app""#) && body.ends_with("<p>flow app</p>"),
        "{body}"
    );
    // The broken ones open nowhere, and the listing says why.
    for site in ["broken", "list"] {
        let (status, body) = status_text(
            call(
                addr,
                Method::POST,
                &format!("/api/apps/{site}/open"),
                Some(&workspace),
                None,
            )
            .await,
        )
        .await;
        assert_eq!(status, 404, "{body}");
        assert!(body.contains("`app:`"), "{body}");
    }
    let (_, body) =
        status_text(call(addr, Method::GET, "/api/apps", Some(&workspace), None).await).await;
    let home: Value = serde_json::from_str(&body).unwrap();
    let errors = home["errors"].to_string();
    assert!(
        errors.contains("site://broken") && errors.contains("site://list/"),
        "{errors}"
    );
    assert!(!errors.contains("site://flow"), "{errors}");
}
