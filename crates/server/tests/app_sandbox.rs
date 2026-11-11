//! Apps run sandboxed (an opaque origin): their requests say `Origin: null` and carry no
//! SameSite=Strict cookie. The one Origin rule: `Origin: null` is answered on an app's own routes
//! (`/a/<token>/...`, its API and its socket) and refused everywhere else (the workspace's socket,
//! the viewer's, its HTTP routes, pairing), even with the workspace's cookie. And
//! `POST /api/web-session`: a browser session of the caller's kind, for clients that set the
//! workspace's cookie themselves (the desktop's panes, the phone's live view).
#![allow(
    clippy::disallowed_methods,
    reason = "tests set up repos and fakes outside the sandbox"
)]

mod support;

use std::path::Path;

use composer_server::api::Credential;
use reqwest::{Client, Method, StatusCode, header};
use serde_json::Value;
use support::*;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A WebSocket handshake to `url` with `headers`: whether it opened.
async fn handshake(url: String, headers: &[(header::HeaderName, String)]) -> bool {
    let mut request = url.into_client_request().unwrap();
    for (name, value) in headers {
        request
            .headers_mut()
            .insert(name.clone(), value.parse().unwrap());
    }
    tokio_tungstenite::connect_async(request).await.is_ok()
}

#[tokio::test]
async fn origin_null_is_answered_on_an_apps_routes_only() {
    let (dir, server, _provider, addr) = start(vec![]).await;
    let sites = dir.path().join("sites");
    write(&sites.join("cal/index.md"), "---\ntitle: Cal\n---\n# Cal\n");
    write(&sites.join("cal/ui/dist/index.html"), "<p>cal</p>");
    write(
        &sites.join("cal/ui/dist/assets/a.js"),
        "d=\"composer-sdk:3\"",
    );
    let app = server.app_tokens.mint(composer_server::apps::AppGrant {
        project: None,
        site: "cal".into(),
        local: None,
        session: None,
    });
    // The workspace's cookie, as a browser that opened a workspace page holds it.
    let session = server.web_sessions.mint(Credential::Local);
    let cookie = (
        header::COOKIE,
        format!("composer_token_{}={session}", addr.port()),
    );
    let null = (header::ORIGIN, "null".to_string());
    let get = |path: &str, headers: Vec<(header::HeaderName, String)>| {
        let mut request = Client::new().get(format!("http://{addr}{path}"));
        for (name, value) in headers {
            request = request.header(name, value);
        }
        async move { request.send().await.unwrap().status() }
    };

    // The cookie works from our own pages...
    assert_eq!(
        get("/api/sessions/x/project/diff", vec![cookie.clone()]).await,
        StatusCode::NOT_FOUND,
        "authenticated: the session doesn't exist"
    );
    assert_eq!(
        get("/api/sites", vec![cookie.clone()]).await,
        StatusCode::OK
    );
    assert!(handshake(format!("ws://{addr}/ws"), std::slice::from_ref(&cookie)).await);
    // ...never from a sandboxed page.
    for path in ["/api/sites", "/api/info", "/view/ws", "/view/", "/ws"] {
        assert_eq!(
            get(path, vec![cookie.clone(), null.clone()]).await,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    assert!(!handshake(format!("ws://{addr}/ws"), &[cookie.clone(), null.clone()]).await);
    assert!(
        !handshake(
            format!("ws://{addr}/ws?token={}", server.token),
            std::slice::from_ref(&null)
        )
        .await
    );
    assert!(
        !handshake(
            format!("ws://{addr}/view/ws"),
            &[cookie.clone(), null.clone()]
        )
        .await
    );
    // Pairing neither.
    let code = server.devices.create_code("phone").unwrap();
    let pair = Client::new()
        .post(format!("http://{addr}/pair"))
        .header(header::ORIGIN, "null")
        .form(&[("code", code.as_str()), ("name", "phone")])
        .send()
        .await
        .unwrap();
    assert_eq!(pair.status(), StatusCode::FORBIDDEN);
    let pair = Client::new()
        .post(format!("http://{addr}/api/pair"))
        .header(header::ORIGIN, "null")
        .json(&serde_json::json!({ "code": code, "name": "phone" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pair.status(), StatusCode::FORBIDDEN);

    // The app's own routes answer it: its page, its API, its socket.
    for path in [
        format!("/a/{app}/"),
        format!("/a/{app}/api/pages/cal/index.md"),
        format!("/a/{app}/api/sites"),
    ] {
        assert_eq!(
            get(&path, vec![null.clone()]).await,
            StatusCode::OK,
            "{path}"
        );
    }
    assert!(
        handshake(
            format!("ws://{addr}/a/{app}/ws"),
            std::slice::from_ref(&null)
        )
        .await
    );
    // Under the app's prefix only its token counts: the cookie opens nothing there.
    assert_eq!(
        get(
            "/a/app_forged/api/sites",
            vec![cookie.clone(), null.clone()]
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert!(
        !handshake(
            format!("ws://{addr}/a/{session}/ws"),
            std::slice::from_ref(&cookie)
        )
        .await
    );
}

#[tokio::test]
async fn a_web_session_is_of_the_callers_kind() {
    let (_dir, server, _provider, addr) = start(vec![]).await;
    let code = server.devices.create_code("phone").unwrap();
    let (_, device) = server.devices.redeem(&code, None).unwrap().unwrap();
    let app = server.app_tokens.mint(composer_server::apps::AppGrant {
        project: None,
        site: "cal".into(),
        local: None,
        session: None,
    });
    let ask = |token: Option<&str>, origin: Option<&str>| {
        let mut request =
            Client::new().request(Method::POST, format!("http://{addr}/api/web-session"));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        async move {
            let response = request.send().await.unwrap();
            let status = response.status();
            (status, response.text().await.unwrap())
        }
    };

    for (token, local) in [(server.token.clone(), true), (device.clone(), false)] {
        let (status, body) = ask(Some(&token), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let session: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            session["cookie_name"],
            format!("composer_token_{}", addr.port())
        );
        assert_eq!(session["path"], "/");
        let value = session["value"].as_str().unwrap();
        assert!(
            value.starts_with("ses_") && !value.contains(&token),
            "{value}"
        );
        let kind = server.credential(value).unwrap();
        assert_eq!(kind == Credential::Local, local, "{kind:?}");
        if !local {
            assert!(matches!(kind, Credential::Device { ref name, .. } if name == "phone"));
        }
        // It is a cookie for the workspace's routes.
        let info = Client::new()
            .get(format!("http://{addr}/api/info"))
            .header(
                header::COOKIE,
                format!("composer_token_{}={value}", addr.port()),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(info.status(), StatusCode::OK);
    }
    // No credential, an app's token, or a sandboxed page: refused.
    assert_eq!(ask(None, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(ask(Some(&app), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        ask(Some(&server.token), Some("null")).await.0,
        StatusCode::FORBIDDEN
    );
}
