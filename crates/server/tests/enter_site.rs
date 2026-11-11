//! `enter_site`: a site's `index.md` joins the prompt and its `tools:` join the base set, until
//! `leave_site`. A tool outside the agent's base list is unknown until the site provides it.

mod support;

use std::sync::Arc;

use composer_core::SiteUrl;
use composer_protocol::ChangedBy;
use composer_server::agent::{AgentDefinition, Site, system_prompt};
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
    server
        .write_page(
            &SiteUrl::new("agents", "baser.md").unwrap(),
            "---\ntitle: baser\ntype: agent\nname: baser\ntools: [open, enter_site, leave_site]\nsites: all\n---\nYou are baser.\n",
            ChangedBy::External,
        )
        .unwrap();

    let agent = Arc::new(AgentDefinition::load(&server.web, "baser").unwrap());
    let session = server.sessions.create("baser", None).unwrap();
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
            agent: agent.clone(),
            site,
            cancel: CancellationToken::new(),
        }
    };

    // The base set doesn't include the site's tool.
    let names = |site: Option<&Site>| {
        server
            .tools
            .specs_for(&agent, site)
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>()
    };
    assert!(names(None).contains(&"open".to_string()));
    assert!(!names(None).contains(&"list_sites".to_string()));
    let lab = Site::load(&server.web, "lab").unwrap();
    assert!(names(Some(&lab)).contains(&"list_sites".to_string()));

    // Outside: unknown.
    let before = server
        .tools
        .call_value(&context(), "list_sites", json!({}))
        .await;
    assert!(before.is_error, "{}", before.text);
    assert!(
        before.text.contains("unknown tool `list_sites`"),
        "{}",
        before.text
    );

    // Enter: persisted and in effect for the very next call.
    let entered = server
        .tools
        .call_value(&context(), "enter_site", json!({ "site": "lab" }))
        .await;
    assert!(!entered.is_error, "{}", entered.text);
    assert_eq!(session.info().site.as_deref(), Some("lab"));
    let inside = server
        .tools
        .call_value(&context(), "list_sites", json!({}))
        .await;
    assert!(!inside.is_error, "{}", inside.text);
    assert!(inside.text.contains("lab"), "{}", inside.text);

    // The prompt has the site's page and tools in place of Home.
    let prompt = {
        let index = server.index.read().unwrap();
        system_prompt(&agent, &server.web, &index, &session.info())
    };
    assert!(prompt.contains("Entered the lab."), "{prompt}");
    assert!(prompt.contains("its tools are: list_sites"), "{prompt}");
    assert!(!prompt.contains("url=\"site://home\""), "{prompt}");

    // Leave: back to the base set.
    let left = server
        .tools
        .call_value(&context(), "leave_site", json!({}))
        .await;
    assert!(!left.is_error, "{}", left.text);
    assert_eq!(session.info().site, None);
    let after = server
        .tools
        .call_value(&context(), "list_sites", json!({}))
        .await;
    assert!(after.is_error, "{}", after.text);

    // A site that doesn't exist is an error, and changes nothing.
    let missing = server
        .tools
        .call_value(&context(), "enter_site", json!({ "site": "nope" }))
        .await;
    assert!(missing.is_error, "{}", missing.text);
    assert_eq!(session.info().site, None);

    // `project: required` refuses without one, then works once it is set.
    write(
        &dir.path().join("sites/game/index.md"),
        "---\ntitle: Game\nproject: required\ntools: []\n---\n# Game\n",
    );
    let gated = server
        .tools
        .call_value(&context(), "enter_site", json!({ "site": "game" }))
        .await;
    assert!(
        gated.is_error && gated.text.contains("needs an active project"),
        "{}",
        gated.text
    );
    session.update_info(&server.hub, |info| {
        info.project = Some(dir.path().display().to_string())
    });
    let entered = server
        .tools
        .call_value(&context(), "enter_site", json!({ "site": "game" }))
        .await;
    assert!(!entered.is_error, "{}", entered.text);
    assert_eq!(session.info().site.as_deref(), Some("game"));

    // An agent may only enter a site it can read.
    server
        .write_page(
            &SiteUrl::new("agents", "locked.md").unwrap(),
            "---\ntitle: locked\ntype: agent\nname: locked\ntools: [open, enter_site]\nsites: [memory]\n---\nLocked.\n",
            ChangedBy::External,
        )
        .unwrap();
    let locked = server.sessions.create("locked", None).unwrap();
    let context = ToolContext {
        server: server.clone(),
        session: locked.clone(),
        agent: Arc::new(AgentDefinition::load(&server.web, "locked").unwrap()),
        site: None,
        cancel: CancellationToken::new(),
    };
    let refused = server
        .tools
        .call_value(&context, "enter_site", json!({ "site": "lab" }))
        .await;
    assert!(
        refused.is_error && refused.text.contains("may not read site `lab`"),
        "{}",
        refused.text
    );
    assert_eq!(locked.info().site, None);
}
