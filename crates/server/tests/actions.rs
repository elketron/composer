//! Site actions: listing, running, and the approve-once-per-version rule. Needs `just`.

mod support;

use composer_protocol::{ApprovalDecision, ClientCommand, CommandResult, ServerMessage};
use composer_server::actions::Justfile;
use composer_server::provider::ModelEvent;
use serde_json::json;
use support::*;

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
async fn seed_recipes_are_preapproved_and_run_in_their_site() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) =
        start(run_turn("orchestration", "new-ticket", &["Device pairing"])).await;
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
    assert!(
        result.contains("site://orchestration/tickets/0001"),
        "{result}"
    );
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(
        ticket.contains("title: Device pairing") && ticket.contains("id: \"0001\""),
        "{ticket}"
    );
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
            tool_call("a", "actions", json!({ "site": "orchestration" })),
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
        results[0].contains("- new-ticket <title> — Create the next ticket")
            && results[0].contains("[approved]"),
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
