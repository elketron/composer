//! Site actions: listing, running, and the approve-once-per-version rule. Needs `just`.

mod support;

use std::sync::Arc;

use composer_protocol::{ApprovalDecision, ClientCommand, CommandResult, ServerMessage};
use composer_server::Server;
use composer_server::actions::{Approvals, Justfile};
use composer_server::agent::AgentDefinition;
use composer_server::provider::ModelEvent;
use composer_server::tools::{CallResult, ToolContext};
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn has_just() -> bool {
    std::process::Command::new("just")
        .arg("--version")
        .output()
        .is_ok()
}

async fn session(socket: &mut Socket) -> String {
    send(
        socket,
        1,
        ClientCommand::SessionCreate {
            agent: "default".into(),
            project: None,
        },
    )
    .await;
    let reply = until(socket, |e| matches!(e, ServerMessage::Ok { id: 1, .. })).await;
    let Some(ServerMessage::Ok {
        result: CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!()
    };
    send(
        socket,
        2,
        ClientCommand::SessionAttach {
            session: session.id.clone(),
            since: None,
        },
    )
    .await;
    until(socket, |e| matches!(e, ServerMessage::Ok { id: 2, .. })).await;
    session.id.clone()
}

/// One turn: the model calls `run` once, then says "done".
fn run_turn(site: &str, recipe: &str, args: &[&str]) -> Vec<Vec<ModelEvent>> {
    vec![
        vec![finished(vec![tool_call(
            "r",
            "run",
            json!({ "site": site, "recipe": recipe, "args": args }),
        )])],
        vec![ModelEvent::Content("done".into()), finished(vec![])],
    ]
}

fn tool_result(events: &[ServerMessage]) -> (String, bool) {
    events
        .iter()
        .find_map(|e| match e {
            ServerMessage::ToolResult {
                result, is_error, ..
            } => Some((result.clone(), *is_error)),
            _ => None,
        })
        .expect("a tool result")
}

async fn ask(socket: &mut Socket, session: &str, id: u64) {
    send(
        socket,
        id,
        ClientCommand::MessageSend {
            session: session.into(),
            text: "go".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
}

#[tokio::test]
async fn a_run_written_loosely_still_runs() {
    if !has_just() {
        return;
    }
    // No site (only `tools` has the recipe), `action` for `recipe`, the args a JSON string.
    let call = json!({ "action": "greet", "args": "[\"Device pairing\"]" });
    let (dir, server, _p, addr) = start(vec![
        vec![finished(vec![tool_call("r", "run", call)])],
        vec![ModelEvent::Content("done".into()), finished(vec![])],
    ])
    .await;
    let site = tools_site(
        &dir,
        "greet title:\n    @echo \"{{quote(title)}}\" > greeted\n",
    );
    approve(&server, &site, "greet").await;
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    ask(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let (result, is_error) = tool_result(&events);
    assert!(!is_error, "{result}");
    let greeted = std::fs::read_to_string(site.join("greeted")).unwrap();
    assert!(greeted.contains("Device pairing"), "{greeted}");
}

#[tokio::test]
async fn seed_recipes_are_preapproved_and_run_in_their_site() {
    if !has_just() {
        return;
    }
    let (_dir, server, _p, addr) = start(run_turn("icm", "pages", &[])).await;
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    ask(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ServerMessage::ApprovalRequest { .. })),
        "seed recipe asked for approval"
    );
    let (result, is_error) = tool_result(&events);
    assert!(!is_error, "{result}");
    // It ran in its own folder: the skill's pages are relative to the site.
    assert!(result.contains("_shared/icm-architect/"), "{result}");
}

#[tokio::test]
async fn approve_once_until_the_recipe_changes() {
    if !has_just() {
        return;
    }
    let mut scripts = run_turn("tools", "hello", &["bob"]);
    scripts.extend(run_turn("tools", "hello", &["ann"]));
    scripts.extend(run_turn("tools", "hello", &["eve"]));
    let (dir, server, _p, addr) = start(scripts).await;
    let site = dir.path().join("sites/tools");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(
        site.join("justfile"),
        "# Greets someone.\nhello name:\n    echo \"hi {{name}} from $COMPOSER_SITE\"\n",
    )
    .unwrap();

    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;

    // First run: asks, we approve, it runs.
    ask(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest {
        request,
        body,
        recipe,
        args,
        ..
    }) = events.last()
    else {
        panic!()
    };
    assert_eq!(
        (recipe.as_str(), args.as_slice()),
        ("hello", &["bob".to_string()][..])
    );
    assert!(
        body.contains("# Greets someone.") && body.contains("echo \"hi {{ name }}"),
        "{body}"
    );
    send(
        &mut socket,
        4,
        ClientCommand::ApprovalRespond {
            request: request.clone(),
            decision: ApprovalDecision::Approve,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::ApprovalResolved { approved: true, .. }))
    );
    let (result, is_error) = tool_result(&events);
    assert!(!is_error, "{result}");
    // just echoes the command (stderr) before its output (stdout): one pipe keeps that order.
    assert_eq!(
        result,
        "tools › hello: ok\necho \"hi bob from $COMPOSER_SITE\"\nhi bob from tools"
    );

    // Second run of the same version: no question.
    ask(&mut socket, &id, 5).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ServerMessage::ApprovalRequest { .. }))
    );
    assert!(tool_result(&events).0.ends_with("hi ann from tools"));

    // Changed recipe: asks again; we deny.
    std::fs::write(
        site.join("justfile"),
        "hello name:\n    echo \"bye {{name}}\"\n",
    )
    .unwrap();
    ask(&mut socket, &id, 6).await;
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest { request, .. }) = events.last() else {
        panic!()
    };
    send(
        &mut socket,
        7,
        ClientCommand::ApprovalRespond {
            request: request.clone(),
            decision: ApprovalDecision::Deny,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let (result, is_error) = tool_result(&events);
    assert!(is_error && result.contains("denied"), "{result}");
}

#[tokio::test]
async fn a_changed_dependency_or_variable_changes_the_hash() {
    if !has_just() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let write = |text: &str| std::fs::write(dir.path().join("justfile"), text).unwrap();
    write("greeting := \"hi\"\nbuild: prepare\n    echo {{greeting}}\nprepare:\n    echo one\n");
    let base = Justfile::load(dir.path()).await.unwrap().hash("build");

    write("greeting := \"hi\"\nbuild: prepare\n    echo {{greeting}}\nprepare:\n    echo two\n");
    let dep_changed = Justfile::load(dir.path()).await.unwrap().hash("build");
    write("greeting := \"yo\"\nbuild: prepare\n    echo {{greeting}}\nprepare:\n    echo one\n");
    let var_changed = Justfile::load(dir.path()).await.unwrap().hash("build");
    write(
        "greeting := \"hi\"\nbuild: prepare\n    echo {{greeting}}\nprepare:\n    echo one\nunrelated:\n    true\n",
    );
    let unrelated_added = Justfile::load(dir.path()).await.unwrap().hash("build");

    assert_ne!(base, dep_changed);
    assert_ne!(base, var_changed);
    assert_eq!(
        base, unrelated_added,
        "adding another recipe must not invalidate approvals"
    );
}

#[tokio::test]
async fn actions_lists_recipes_with_approval_state() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![
        vec![finished(vec![
            tool_call("a", "actions", json!({ "site": "icm" })),
            tool_call("b", "actions", json!({ "site": "tools" })),
        ])],
        vec![ModelEvent::Content("ok".into()), finished(vec![])],
    ])
    .await;
    let site = dir.path().join("sites/tools");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(
        site.join("justfile"),
        "# Says hi.\nhi who='world':\n    echo hi {{who}}\n_helper:\n    true\n",
    )
    .unwrap();

    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    ask(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let results: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            ServerMessage::ToolResult { result, .. } => Some(result.clone()),
            _ => None,
        })
        .collect();
    assert!(
        results[0].contains("- pages — list the skill pages") && results[0].contains("[approved]"),
        "{}",
        results[0]
    );
    assert!(
        results[1].contains("- hi <who='world'> — Says hi. [needs the user's approval]"),
        "{}",
        results[1]
    );
    assert!(!results[1].contains("_helper"));
}

/// A site `tools` with this justfile; returns its folder.
fn tools_site(dir: &tempfile::TempDir, justfile: &str) -> std::path::PathBuf {
    let site = dir.path().join("sites/tools");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(site.join("justfile"), justfile).unwrap();
    site
}

/// Approves the current version of a recipe, as a user's earlier answer would have.
async fn approve(server: &Server, site: &std::path::Path, recipe: &str) {
    let hash = Justfile::load(site).await.unwrap().hash(recipe);
    server.approvals.approve("tools", recipe, &hash).unwrap();
}

/// The agent's `run` of `recipe` in site `tools` with `args`, outside a turn.
async fn run(server: &Arc<Server>, recipe: &str, args: &[&str]) -> CallResult {
    let session = server.sessions.create("default", None).unwrap();
    let context = ToolContext {
        server: server.clone(),
        session,
        agent: Arc::new(AgentDefinition::load(&server.web, "default").unwrap()),
        site: None,
        cancel: CancellationToken::new(),
    };
    server
        .tools
        .call_value(
            &context,
            "run",
            json!({ "site": "tools", "recipe": recipe, "args": args }),
        )
        .await
}

#[tokio::test]
async fn a_new_version_forgets_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let approvals = Approvals::load(&dir.path().join("approvals.json")).unwrap();
    approvals.approve("s", "r", "v1").unwrap();
    assert!(approvals.is_approved("s", "r", "v1"));

    approvals.approve("s", "r", "v2").unwrap();
    assert!(!approvals.is_approved("s", "r", "v1"));
    assert!(approvals.is_approved("s", "r", "v2"));
}

#[test]
fn approvals_given_to_apps_never_count() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approvals.json");
    std::fs::write(
        &path,
        r#"[{"site":"s","recipe":"r","hash":"v1","caller":"app","approved_at":"x"},
            {"site":"s","recipe":"q","hash":"v1","approved_at":"x"}]"#,
    )
    .unwrap();
    let approvals = Approvals::load(&path).unwrap();
    assert!(!approvals.is_approved("s", "r", "v1"));
    assert!(approvals.is_approved("s", "q", "v1"));
}

#[test]
fn saving_approvals_replaces_the_file_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approvals.json");
    let approvals = Approvals::load(&path).unwrap();
    approvals.approve("s", "r", "v1").unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    // A second name for the old file: writing in place would change it too.
    let old = dir.path().join("old");
    std::fs::hard_link(&path, &old).unwrap();

    approvals.approve("s", "r", "v2").unwrap();
    assert_eq!(std::fs::read_to_string(&old).unwrap(), before);
    assert!(Approvals::load(&path).unwrap().is_approved("s", "r", "v2"));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn variadic_recipes_take_any_number_of_arguments_and_no_other_recipes_run() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, _addr) = start(vec![]).await;
    let site = tools_site(
        &dir,
        "set positional-arguments\n\
         star *rest:\n    echo \"$#\" > star.out\n\
         plus +rest:\n    echo \"$#\" > plus.out\n\
         mix a *rest:\n    echo \"$#\" > mix.out\n\
         dflt a b='d':\n    echo \"$#\" > dflt.out\n\
         none:\n    touch none.out\n\
         other:\n    touch other.out\n",
    );
    for recipe in ["star", "plus", "mix", "dflt", "none"] {
        approve(&server, &site, recipe).await;
    }
    let ok = |result: CallResult| !result.is_error;
    let count = |recipe: &str| {
        std::fs::read_to_string(site.join(format!("{recipe}.out")))
            .unwrap()
            .trim()
            .to_string()
    };

    assert!(ok(run(&server, "star", &["a", "b", "other"]).await));
    assert_eq!(count("star"), "3");
    assert!(ok(run(&server, "plus", &["a", "b"]).await));
    assert_eq!(count("plus"), "2");
    assert!(ok(run(&server, "mix", &["a", "b", "c"]).await));
    assert_eq!(count("mix"), "3");
    assert!(ok(run(&server, "dflt", &["a"]).await));
    assert!(ok(run(&server, "dflt", &["a", "b"]).await));
    // Not variadic: the argument too many would be a recipe to `just`.
    assert!(!ok(run(&server, "dflt", &["a", "b", "other"]).await));
    assert!(!ok(run(&server, "none", &["other"]).await));
    assert!(!site.join("none.out").exists());
    assert!(
        !site.join("other.out").exists(),
        "another recipe ran on the way"
    );
}

#[tokio::test]
async fn the_sh_escape_hatch_takes_one_command_with_quotes_and_newlines() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, _addr) = start(vec![]).await;
    let site = tools_site(
        &dir,
        "sh cmd:\n    @cd \"$COMPOSER_PROJECT\" && sh -c {{quote(cmd)}}\n",
    );
    approve(&server, &site, "sh").await;
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("README"), "hi\n").unwrap();
    let session = server
        .sessions
        .create("default", Some(project.path().display().to_string()))
        .unwrap();
    let context = ToolContext {
        server: server.clone(),
        session,
        agent: Arc::new(AgentDefinition::load(&server.web, "default").unwrap()),
        site: None,
        cancel: CancellationToken::new(),
    };
    let result = server
        .tools
        .call_value(
            &context,
            "run",
            json!({
                "site": "tools",
                "recipe": "sh",
                "args": ["-c", "printf '%s\\n' \"a b\"; printf 'c\\nd\\n'"],
            }),
        )
        .await;
    assert!(!result.is_error, "{}", result.text);
    assert!(
        result.text.contains("a b") && result.text.contains("c\nd"),
        "{}",
        result.text
    );
}
