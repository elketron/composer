//! Webhooks (`[hooks.<name>]`, `POST /api/hooks/<name>`): only the hook's own token opens one; the
//! run is of the hook's kind and the reply carries its result. Unstored, nothing is written under
//! `state/sessions`; stored, the reply names the session.

mod support;

use std::sync::Arc;

use composer_protocol::http::HookReply;
use composer_protocol::{ChangedBy, RunKind};
use composer_server::provider::{ModelEvent, ModelMessage};
use composer_server::runs::HookConfig;
use composer_server::session::{MemorySink, RunSink};
use composer_server::tools::ToolContext;
use composer_server::{Config, Server};
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

const TOKEN: &str = "hook-secret";

fn hook(kind: RunKind, store: bool) -> HookConfig {
    HookConfig {
        token_file: String::new(),
        token: TOKEN.into(),
        kind,
        site: Some("prompts".into()),
        folder: Some("krea".into()),
        prompt: "Improve: {{prompt}}".into(),
        store,
        timeout_secs: 30,
    }
}

fn config(hooks: Vec<(&str, HookConfig)>) -> Config {
    let mut config = Config::defaults();
    for (name, hook) in hooks {
        config.hooks.insert(name.into(), hook);
    }
    config
}

/// A site `prompts` with a workflow folder `krea` and its instructions.
fn write_prompts_site(server: &Server) {
    std::fs::create_dir_all(server.home.sites().join("prompts")).unwrap();
    let page = |path: &str, text: &str| {
        let url = composer_core::SiteUrl::new("prompts", path).unwrap();
        server.write_page(&url, text, ChangedBy::External).unwrap();
    };
    page("index.md", "---\ntitle: Prompts\n---\nThe prompts site.\n");
    page(
        "krea/CONTEXT.md",
        "---\ntitle: krea\n---\nRewrite the prompt for krea2. Answer with the prompt only.\n",
    );
}

async fn post(
    addr: std::net::SocketAddr,
    name: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> reqwest::Response {
    let mut request = reqwest::Client::new()
        .post(format!("http://{addr}/api/hooks/{name}"))
        .json(&body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    request.send().await.unwrap()
}

fn stored_sessions(dir: &tempfile::TempDir) -> usize {
    std::fs::read_dir(dir.path().join("state/sessions"))
        .map(|entries| entries.count())
        .unwrap_or(0)
}

#[tokio::test]
async fn only_the_hooks_own_token_opens_it() {
    let (_dir, server, provider, addr) = start_with(
        config(vec![("krea", hook(RunKind::SingleShot, false))]),
        vec![],
    )
    .await;
    let body = json!({ "prompt": "a cat" });
    assert_eq!(
        post(addr, "nope", Some(TOKEN), body.clone()).await.status(),
        404
    );
    assert_eq!(post(addr, "krea", None, body.clone()).await.status(), 401);
    assert_eq!(
        post(addr, "krea", Some("wrong"), body.clone())
            .await
            .status(),
        401
    );
    // The local token opens every other route, not this one.
    assert_eq!(
        post(addr, "krea", Some(&server.token), body).await.status(),
        401
    );
    assert!(provider.requests().is_empty(), "nothing ran");
}

#[tokio::test]
async fn a_single_shot_hook_answers_and_stores_nothing() {
    let (dir, server, provider, addr) = start_with(
        config(vec![("krea", hook(RunKind::SingleShot, false))]),
        vec![vec![
            ModelEvent::Content("a fluffy cat, golden hour".into()),
            finished(vec![]),
        ]],
    )
    .await;
    write_prompts_site(&server);
    let response = post(addr, "krea", Some(TOKEN), json!({ "prompt": "a cat" })).await;
    assert_eq!(response.status(), 200);
    let reply: HookReply = response.json().await.unwrap();
    assert_eq!(
        reply,
        HookReply {
            value: json!("a fluffy cat, golden hour"),
            session: None
        }
    );

    // One call: the folder's instructions are the whole system prompt, the input the message.
    let requests = provider.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].tools.is_empty());
    match &requests[0].messages[..] {
        [
            ModelMessage::System { content },
            ModelMessage::User { content: input, .. },
        ] => {
            assert_eq!(
                content,
                "Rewrite the prompt for krea2. Answer with the prompt only."
            );
            assert_eq!(input, "Improve: a cat");
        }
        other => panic!("{other:?}"),
    }
    assert!(server.sessions.list().is_empty());
    assert_eq!(stored_sessions(&dir), 0);
}

#[tokio::test]
async fn a_stored_hook_run_is_a_session() {
    let (dir, server, _provider, addr) = start_with(
        config(vec![("krea", hook(RunKind::SingleShot, true))]),
        vec![vec![ModelEvent::Content("better".into()), finished(vec![])]],
    )
    .await;
    write_prompts_site(&server);
    let reply: HookReply = post(addr, "krea", Some(TOKEN), json!({ "prompt": "a cat" }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(reply.value, json!("better"));
    let id = reply.session.expect("a stored run names its session");
    assert!(dir.path().join("state/sessions").join(&id).is_dir());
    let info = server.sessions.get(&id).unwrap().info();
    assert_eq!(info.kind, RunKind::SingleShot);
    assert_eq!(info.site.as_deref(), Some("prompts"));
    assert_eq!(info.folder.as_deref(), Some("krea"));
    assert_eq!(stored_sessions(&dir), 1);
}

#[tokio::test]
async fn a_workflow_hook_returns_its_finish_value() {
    let (dir, server, provider, addr) = start_with(
        config(vec![(
            "scenes",
            HookConfig {
                prompt: String::new(),
                ..hook(RunKind::Workflow, false)
            },
        )]),
        vec![
            vec![finished(vec![tool_call(
                "c1",
                "open",
                json!({ "url": "site://prompts/krea/CONTEXT.md" }),
            )])],
            vec![finished(vec![tool_call(
                "c2",
                "finish",
                json!({ "value": { "scenes": ["a", "b"] } }),
            )])],
        ],
    )
    .await;
    write_prompts_site(&server);
    let reply: HookReply = post(addr, "scenes", Some(TOKEN), json!("a story"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(reply.value, json!({ "scenes": ["a", "b"] }));
    assert_eq!(reply.session, None);
    // A workflow run keeps composer's prompt, with the folder's instructions in it.
    let ModelMessage::System { content } = &provider.requests()[0].messages[0] else {
        panic!()
    };
    assert!(
        content.contains("site://prompts/krea/CONTEXT.md"),
        "{content}"
    );
    assert!(
        content.contains("Rewrite the prompt for krea2"),
        "{content}"
    );
    assert_eq!(stored_sessions(&dir), 0);
}

#[tokio::test]
async fn an_agentic_hook_returns_the_final_answer() {
    let (_dir, server, _provider, addr) = start_with(
        config(vec![("agent", hook(RunKind::Agentic, false))]),
        vec![
            vec![finished(vec![tool_call(
                "c1",
                "search",
                json!({ "query": "krea" }),
            )])],
            vec![ModelEvent::Content("all done".into()), finished(vec![])],
        ],
    )
    .await;
    write_prompts_site(&server);
    let reply: HookReply = post(addr, "agent", Some(TOKEN), json!({ "prompt": "go" }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(reply.value, json!("all done"));
}

#[tokio::test]
async fn a_hook_with_a_folder_that_does_not_exist_fails_clearly() {
    let (_dir, server, provider, addr) = start_with(
        config(vec![(
            "krea",
            HookConfig {
                folder: Some("missing".into()),
                ..hook(RunKind::SingleShot, false)
            },
        )]),
        vec![],
    )
    .await;
    write_prompts_site(&server);
    let response = post(addr, "krea", Some(TOKEN), json!({ "prompt": "x" })).await;
    assert_eq!(response.status(), 500);
    let text = response.text().await.unwrap();
    assert!(text.contains("no CONTEXT.md or index.md"), "{text}");
    assert!(provider.requests().is_empty());
}

/// An approval in a run that is not stored stops the run with a clear error: nobody could answer.
#[tokio::test]
async fn an_approval_in_an_unstored_run_stops_it() {
    let (_dir, server, _provider, _addr) = start_with(Config::defaults(), vec![]).await;
    let sink = Arc::new(MemorySink::new("go", None));
    let context = ToolContext {
        server: server.clone(),
        run: sink.clone(),
        kind: RunKind::Agentic,
        only: None,
        site: None,
        cancel: CancellationToken::new(),
    };
    let error = context.approval_session("`fake__boom`").unwrap_err();
    assert!(error.contains("needs the user's approval"), "{error}");
    assert_eq!(sink.aborted(), Some(error));
}
