//! Pairing a device: trade a one-time code (from `composer pair`) for a device token.
//! These routes need no token; the code is the credential.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::api::auth::{is_https, same_origin, set_cookie};
use crate::server::Server;

/// A paired browser stays signed in for a year (revoke with `composer devices revoke`).
const DEVICE_COOKIE_SECONDS: u64 = 365 * 24 * 3600;

#[derive(Debug, Deserialize)]
pub struct PairForm {
    pub code: String,
    #[serde(default)]
    pub name: String,
}

/// `GET /pair?code=...`: asks to confirm, so a link preview can't use up the code.
pub async fn pair_page(Query(params): Query<HashMap<String, String>>) -> Html<String> {
    let code = params.get("code").cloned().unwrap_or_default();
    let code = html_escape(&code);
    page(&format!(
        "<h1>Pair this device</h1>\
         <p>It gets its own token for composer. You can revoke it with <code>composer devices revoke</code>.</p>\
         <form method=\"post\" action=\"/pair\">\
           <label>code<input name=\"code\" value=\"{code}\" autocomplete=\"off\" required></label>\
           <label>device name (optional)<input name=\"name\" placeholder=\"the name given to composer pair\"></label>\
           <button type=\"submit\">Pair</button>\
         </form>"
    ))
}

/// `POST /pair` (form): sets the device cookie and opens the chat app.
pub async fn pair_submit(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Form(form): Form<PairForm>,
) -> Response {
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
        && !same_origin(origin, &headers)
    {
        return (StatusCode::FORBIDDEN, "cross-origin request").into_response();
    }
    match server.devices.redeem(&form.code, Some(&form.name)) {
        Ok(Some((name, token))) => {
            tracing::info!("paired device `{name}`");
            let mut response = (StatusCode::SEE_OTHER, [(header::LOCATION, "/chat/")]).into_response();
            set_cookie(&mut response, &token, is_https(&headers), Some(DEVICE_COOKIE_SECONDS));
            response
        }
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            page("<h1>That code didn't work</h1><p>It is wrong, already used, or older than 10 minutes. Run <code>composer pair</code> for a new one.</p>"),
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// `POST /api/pair` (JSON `{code, name}`): for `composer login` on another machine.
pub async fn pair_api(State(server): State<Arc<Server>>, Json(form): Json<PairForm>) -> Response {
    match server.devices.redeem(&form.code, Some(&form.name)) {
        Ok(Some((name, token))) => Json(json!({ "name": name, "token": token })).into_response(),
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or expired code" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

fn page(body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html><head><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\">\
         <title>composer · pair</title><style>\
         body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0c0d0f;color:#d5d8de;font:15px/1.5 system-ui,sans-serif}}\
         main{{width:min(360px,90vw)}}h1{{font-size:20px;font-weight:600;margin:0 0 8px}}p{{color:#868c96;margin:0 0 20px}}\
         code{{font:13px ui-monospace,monospace;color:#d5d8de}}label{{display:block;font:12px ui-monospace,monospace;color:#50555e;margin-bottom:12px}}\
         input{{display:block;width:100%;box-sizing:border-box;margin-top:4px;height:42px;padding:0 12px;border-radius:5px;\
         border:1px solid #363a42;background:#121317;color:#d5d8de;font:16px ui-monospace,monospace}}\
         button{{width:100%;height:44px;border:0;border-radius:5px;background:#8fb3e8;color:#0c0d0f;font:600 15px system-ui;margin-top:8px}}\
         </style></head><body><main>{body}</main></body></html>"
    ))
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
