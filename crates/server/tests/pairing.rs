//! Remote access: pairing devices and their tokens, behind `tailscale serve`.

mod support;

use serde_json::json;
use support::*;

fn no_redirects() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn phone_pairs_through_the_page_and_stays_signed_in() {
    let (_dir, server, _p, addr) = start(vec![]).await;
    let base = format!("http://{addr}");
    let client = no_redirects();
    let code = server.devices.create_code("phone").unwrap();

    // The page needs no token and doesn't use the code by itself.
    let page = client
        .get(format!("{base}/pair?code={code}"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    assert!(
        page.text()
            .await
            .unwrap()
            .contains(&format!("value=\"{code}\""))
    );

    // Confirming behind tailscale serve: https origin, forwarded host and proto.
    let paired = client
        .post(format!("{base}/pair"))
        .header("origin", "https://pc.tailnet.ts.net")
        .header("x-forwarded-host", "pc.tailnet.ts.net")
        .header("x-forwarded-proto", "https")
        .form(&[("code", code.as_str()), ("name", "")])
        .send()
        .await
        .unwrap();
    assert_eq!(paired.status(), 303);
    assert_eq!(paired.headers()["location"], "/chat/");
    let cookie = paired.headers()["set-cookie"].to_str().unwrap().to_string();
    assert!(
        cookie.contains("Secure") && cookie.contains("HttpOnly") && cookie.contains("Max-Age="),
        "{cookie}"
    );
    let cookie = cookie.split(';').next().unwrap().to_string();

    let sites = client
        .get(format!("{base}/api/sites"))
        .header("cookie", &cookie)
        .header("origin", "https://pc.tailnet.ts.net")
        .header("x-forwarded-host", "pc.tailnet.ts.net")
        .send()
        .await
        .unwrap();
    assert_eq!(sites.status(), 200, "device cookie + forwarded origin");

    // The code is spent.
    let again = client
        .post(format!("{base}/pair"))
        .form(&[("code", code.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 401);

    // Revoking cuts it off.
    assert!(server.devices.revoke("phone").unwrap());
    let revoked = client
        .get(format!("{base}/api/sites"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), 401);
}

#[tokio::test]
async fn computer_logs_in_with_the_api() {
    let (_dir, server, _p, addr) = start(vec![]).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();
    let code = server.devices.create_code("laptop").unwrap();

    let wrong = client
        .post(format!("{base}/api/pair"))
        .json(&json!({ "code": "AAAA-AAAA" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);

    let login: serde_json::Value = client
        .post(format!("{base}/api/pair"))
        .json(&json!({ "code": code, "name": "work-laptop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(login["name"], "work-laptop");
    let token = login["token"].as_str().unwrap();

    // The device token works for HTTP and the WebSocket like the local one.
    let pages = client
        .get(format!("{base}/api/pages/home"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(pages.status(), 200);
    let mut socket = connect(addr, token).await;
    send(
        &mut socket,
        1,
        composer_protocol::ClientCommand::SessionList,
    )
    .await;
    until(&mut socket, |e| {
        matches!(e, composer_protocol::ServerMessage::Ok { id: 1, .. })
    })
    .await;

    // A cross-site request is still refused, even with a valid token.
    let cross = client
        .get(format!("{base}/api/sites"))
        .bearer_auth(token)
        .header("origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status(), 403);
}
