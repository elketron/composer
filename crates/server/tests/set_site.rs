//! `set_site`: a site's `index.md` joins the prompt in place of Home and its `tools:` bring the
//! site tools, until `set_site` with no site. A run's kind decides which tools it has.

mod support;

use std::sync::Arc;

use composer_protocol::RunKind;
use composer_server::agent::{RunSettings, Site, system_prompt};
use composer_server::tools::{READ_ONLY_TOOLS, ToolContext};
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
        "---\ntitle: Lab\ntools: [new_site]\n---\n# Lab\n\nEntered the lab.\n",
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
            run: server.sink(&session),
            kind: RunKind::Agentic,
            only: None,
            site,
            cancel: CancellationToken::new(),
        }
    };
    let names = |site: Option<&Site>| {
        server
            .tools
            .specs_for(RunKind::Agentic, site, None)
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>()
    };

    // Outside: the entry tools, not the site's.
    assert!(names(None).contains(&"set_site".to_string()));
    assert!(!names(None).contains(&"new_site".to_string()));
    let lab = Site::load(&server.web, "lab").unwrap();
    assert!(names(Some(&lab)).contains(&"new_site".to_string()));
    let before = server
        .tools
        .call_value(&context(), "new_site", json!({ "name": "x" }))
        .await;
    assert!(
        before.is_error && before.text.contains("unknown tool `new_site`"),
        "{}",
        before.text
    );

    // Set: persisted and in effect for the very next call.
    let entered = server
        .tools
        .call_value(&context(), "set_site", json!({ "site": "lab" }))
        .await;
    assert!(!entered.is_error, "{}", entered.text);
    assert!(entered.text.contains("Entered lab"), "{}", entered.text);
    assert_eq!(session.info().site.as_deref(), Some("lab"));
    assert!(
        context()
            .site
            .is_some_and(|site| site.allows_tool("new_site"))
    );

    // The prompt has the site's page and tools in place of Home.
    let prompt = {
        let index = server.index.read().unwrap();
        system_prompt(
            &settings,
            &server.web,
            &index,
            session.info().site.as_deref(),
            session.id(),
        )
    };
    assert!(prompt.contains("Entered the lab."), "{prompt}");
    assert!(prompt.contains("its tools are: new_site"), "{prompt}");
    assert!(!prompt.contains("url=\"site://home\""), "{prompt}");

    // `null` leaves; again, it was in none.
    let left = server
        .tools
        .call_value(&context(), "set_site", json!({ "site": null }))
        .await;
    assert!(
        !left.is_error && left.text.contains("Left site lab"),
        "{}",
        left.text
    );
    assert_eq!(session.info().site, None);
    let again = server
        .tools
        .call_value(&context(), "set_site", json!({}))
        .await;
    assert!(again.text.contains("not in a site"), "{}", again.text);

    // A site that doesn't exist is an error, and changes nothing.
    let missing = server
        .tools
        .call_value(&context(), "set_site", json!({ "site": "nope" }))
        .await;
    assert!(missing.is_error, "{}", missing.text);
    assert_eq!(session.info().site, None);
}

#[tokio::test]
async fn a_runs_kind_decides_its_tools() {
    let (dir, server, _provider, _addr) = start(vec![]).await;
    write(
        &dir.path().join("sites/lab/index.md"),
        "---\ntitle: Lab\ntools: [new_site]\n---\n# Lab\n",
    );
    let lab = Site::load(&server.web, "lab").unwrap();
    let names = |kind: RunKind| {
        let mut names: Vec<String> = server
            .tools
            .specs_for(kind, Some(&lab), None)
            .into_iter()
            .map(|spec| spec.name)
            .collect();
        names.sort();
        names
    };

    // A single shot has none; a workflow run exactly the read-only ones, the site's tools
    // not among them; an agentic run every tool but `finish`.
    assert!(names(RunKind::SingleShot).is_empty());
    let mut read_only: Vec<String> = READ_ONLY_TOOLS.iter().map(|t| t.to_string()).collect();
    read_only.sort();
    let workflow: Vec<String> = names(RunKind::Workflow);
    for tool in &workflow {
        assert!(read_only.contains(tool), "{tool} in a workflow run");
    }
    for tool in ["open", "search", "list_sites", "set_site"] {
        assert!(workflow.contains(&tool.to_string()), "{tool}: {workflow:?}");
    }
    let agentic = names(RunKind::Agentic);
    for tool in [
        "write",
        "edit",
        "run",
        "terminal_send",
        "process_start",
        "process_stop",
        "process_list",
        "new_site",
        "watch",
        "set_site",
    ] {
        assert!(agentic.contains(&tool.to_string()), "{tool}: {agentic:?}");
    }
    assert!(!agentic.contains(&"finish".to_string()));

    // A call outside the kind's tools is unknown.
    let session = server
        .sessions
        .create(RunKind::Workflow, None, None)
        .unwrap();
    let context = ToolContext {
        server: server.clone(),
        run: server.sink(&session),
        kind: RunKind::Workflow,
        only: None,
        site: None,
        cancel: CancellationToken::new(),
    };
    let refused = server
        .tools
        .call_value(
            &context,
            "write",
            json!({ "url": "site://memory/x", "content": "x" }),
        )
        .await;
    assert!(
        refused.is_error && refused.text.contains("unknown tool `write`"),
        "{}",
        refused.text
    );
}
