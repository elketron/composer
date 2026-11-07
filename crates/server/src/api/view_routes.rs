//! Read-only HTML views for clients that show web content (the desktop's page and changes tabs,
//! ticket 0183): `/view/<site>/<path>` renders a page's markdown, `/view/changes/<project>` a
//! project's changes. Behind the workspace token like the rest of the API.
//!
//! Pages are written by agents, so a view never runs what's in them: raw HTML in the markdown is
//! shown as text, and the response forbids scripts (CSP). Links stay as written (`site://...`,
//! relative): the client opens them as tabs.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use composer_core::SiteUrl;
use pulldown_cmark::{Event, Options, Parser, html};

use crate::api::api_error::ApiError;
use crate::server::Server;

/// No scripts, no plugins, no outside requests: styles inline, images from the server or data.
const POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src 'self' data:; base-uri 'none'; form-action 'none'";

const STYLE: &str = "
:root { color-scheme: light dark; --fg: #1d1d22; --muted: #6b6b75; --bg: #fafafa; --line: #e2e2e8;
        --add: #1a7f37; --del: #cf222e; --code: #f0f0f3; }
@media (prefers-color-scheme: dark) {
  :root { --fg: #d7d7dd; --muted: #85858f; --bg: #0c0c0f; --line: #25252e; --add: #3fb950;
          --del: #f85149; --code: #16161b; } }
body { margin: 0; padding: 24px 32px; background: var(--bg); color: var(--fg);
       font: 15px/1.6 system-ui, sans-serif; max-width: 860px; }
a { color: inherit; text-decoration-color: var(--muted); }
pre, code { font-family: 'JetBrains Mono', ui-monospace, monospace; font-size: 13px; }
pre { background: var(--code); padding: 12px; overflow-x: auto; border-radius: 6px; }
table { border-collapse: collapse; } td, th { border: 1px solid var(--line); padding: 4px 8px; }
.meta { color: var(--muted); font-size: 13px; }
.add { color: var(--add); } .del { color: var(--del); } .hunk { color: var(--muted); }
";

pub async fn page(
    State(server): State<Arc<Server>>,
    Path((site, path)): Path<(String, String)>,
) -> Response {
    shown(
        SiteUrl::new(&site, &path)
            .map_err(ApiError::from)
            .and_then(|url| render_page(&server, url)),
    )
}

pub async fn site_index(State(server): State<Arc<Server>>, Path(site): Path<String>) -> Response {
    shown(
        SiteUrl::new(&site, "")
            .map_err(ApiError::from)
            .and_then(|url| render_page(&server, url)),
    )
}

fn render_page(server: &Server, url: SiteUrl) -> Result<Response, ApiError> {
    let page = server.web.read_page(&url)?;
    let body = markdown(page.body());
    let meta = format!("<p class=\"meta\">{}</p>", escape(&page.url.to_string()));
    // The page's own markdown has its heading.
    Ok(document(&page.title(), None, &(meta + &body)))
}

pub async fn changes(State(server): State<Arc<Server>>, Path(project): Path<String>) -> Response {
    shown(render_changes(&server, &project).await)
}

/// A view or its error, both as pages (a browser shows them).
fn shown(view: Result<Response, ApiError>) -> Response {
    view.unwrap_or_else(|ApiError(status, message)| {
        let mut page = document(
            "Not shown",
            Some("Not shown"),
            &format!("<p>{}</p>", escape(&message)),
        );
        *page.status_mut() = status;
        page
    })
}

async fn render_changes(server: &Server, project: &str) -> Result<Response, ApiError> {
    let (name, folder) = crate::apps::find_project_by(server, project)
        .map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
    let folder = server
        .see_project(&folder.to_string_lossy())
        .map_err(|e| ApiError(StatusCode::FORBIDDEN, e))?;
    let diff = crate::project::project_diff_plain(std::path::Path::new(&folder), None)
        .await
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let mut out = format!(
        "<p class=\"meta\">{} · {}</p>",
        escape(&diff.path),
        escape(diff.branch.as_deref().unwrap_or("no branch"))
    );
    if diff.files.is_empty() {
        out.push_str("<p>No changes.</p>");
    } else {
        out.push_str("<table>");
        for file in &diff.files {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"add\">+{}</td><td class=\"del\">-{}</td></tr>",
                escape(&file.status),
                escape(&file.path),
                file.added,
                file.removed
            ));
        }
        out.push_str("</table><pre>");
        for line in diff.diff.lines() {
            let class = match line.chars().next() {
                Some('+') if !line.starts_with("+++") => "add",
                Some('-') if !line.starts_with("---") => "del",
                Some('@') => "hunk",
                _ => "",
            };
            out.push_str(&format!(
                "<span class=\"{class}\">{}</span>\n",
                escape(line)
            ));
        }
        out.push_str("</pre>");
    }
    let title = format!("changes · {}", name.as_str());
    Ok(document(&title, Some(&title), &out))
}

/// Markdown to HTML, with any raw HTML in it shown as text.
fn markdown(text: &str) -> String {
    let parser = Parser::new_ext(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// A whole page: `title` for the tab, `heading` drawn above `body` when given.
fn document(title: &str, heading: Option<&str>, body: &str) -> Response {
    let heading = heading
        .map(|h| format!("<h1>{}</h1>", escape(h)))
        .unwrap_or_default();
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>{STYLE}</style></head><body>{heading}{body}</body></html>",
        escape(title)
    );
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_SECURITY_POLICY, POLICY),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        html,
    )
        .into_response()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_html_is_text_and_links_stay() {
        let out = markdown(
            "hi <script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[next](site://plans/x.md)",
        );
        assert!(!out.contains("<script"), "{out}");
        assert!(!out.contains("<img"), "{out}");
        assert!(out.contains("&lt;script&gt;"), "{out}");
        assert!(out.contains("href=\"site://plans/x.md\""), "{out}");
        assert_eq!(
            escape("<a href=\"x\">&"),
            "&lt;a href=&quot;x&quot;&gt;&amp;"
        );
    }
}
