//! `enter_site`: a site's `index.md` joins the prompt in place of Home, until `leave_site`.

mod support;

use std::sync::Arc;

use composer_protocol::RunKind;
use composer_server::agent::{RunSettings, Site, system_prompt};
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn a_site_brings_its_page_and_tools() {
    let (dir, server, _provider, _addr) = start(vec![]).await;
    write(
        &dir.path().join("sites/lab/index.md"),
        "---\ntitle: Lab\ntools: [list_sites]\n---\n# Lab\n\nEntered the lab.\n",
    );
    let settings = RunSettings::new(RunKind::Agentic);
    let session = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();
    let context = || {
        let site = session
            .info()
            .site
            .as_deref()
            .and_then(|name| Site::load(&server.web, name).ok())
            .map(Arc::new);
        ToolContext {
            server: server.clone(),
            session: session.clone(),
            kind: RunKind::Agentic,
            only: None,
            site,
            cancel: CancellationToken::new(),
        }
    };

    // Enter: persisted and in effect for the very next call.
    let entered = server
        .tools
        .call_value(&context(), "enter_site", json!({ "site": "lab" }))
        .await;
    assert!(!entered.is_error, "{}", entered.text);
    assert_eq!(session.info().site.as_deref(), Some("lab"));

    // The prompt has the site's page in place of Home.
    let prompt = {
        let index = server.index.read().unwrap();
        system_prompt(&settings, &server.web, &index, &session.info())
    };
    assert!(prompt.contains("Entered the lab."), "{prompt}");
    assert!(prompt.contains("its tools are: list_sites"), "{prompt}");
    assert!(!prompt.contains("url=\"site://home\""), "{prompt}");

    // Leave.
    let left = server
        .tools
        .call_value(&context(), "leave_site", json!({}))
        .await;
    assert!(!left.is_error, "{}", left.text);
    assert_eq!(session.info().site, None);

    // A site that doesn't exist is an error, and changes nothing.
    let missing = server
        .tools
        .call_value(&context(), "enter_site", json!({ "site": "nope" }))
        .await;
    assert!(missing.is_error, "{}", missing.text);
    assert_eq!(session.info().site, None);
}
