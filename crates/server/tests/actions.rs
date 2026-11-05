//! Site actions: listing, running, and the approve-once-per-version rule. Needs `just`.

mod support;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::routing::post;
use composer_protocol::{
    ApprovalDecision, ClientCommand, CommandResult, ServerMessage, SessionInfo, TurnState,
};
use composer_server::Server;
use composer_server::actions::{Approvals, Caller, Justfile, ensure_approved};
use composer_server::provider::ModelEvent;
use composer_server::session::Session;
use composer_server::worker::Verdict;
use composer_server::{Config, Home, init_home, update_home};
use serde_json::{Value, json};
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
async fn a_run_written_loosely_still_runs() {
    if !has_just() {
        return;
    }
    // No site (only orchestration has the recipe), `action` for `recipe`, the args a JSON string.
    let call = json!({ "action": "new-ticket", "args": "[\"Device pairing\"]" });
    let (dir, server, _p, addr) = start(vec![
        vec![finished(vec![tool_call("r", "run", call)])],
        vec![ModelEvent::Content("done".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    ask(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let (result, is_error) = tool_result(&events);
    assert!(!is_error, "{result}");
    let ticket =
        std::fs::read_to_string(dir.path().join("sites/orchestration/tickets/0001.md")).unwrap();
    assert!(ticket.contains("title: Device pairing"), "{ticket}");
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

async fn http(
    server: &composer_server::Server,
    addr: std::net::SocketAddr,
    path: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let client = reqwest::Client::new();
    let url = format!("http://{addr}{path}");
    let request = match body {
        Some(body) => client.post(url).json(&body),
        None => client.get(url),
    };
    let response = request.bearer_auth(&server.token).send().await.unwrap();
    (
        response.status().as_u16(),
        response.json().await.unwrap_or_default(),
    )
}

/// Waits for an approval question and gives its request id.
async fn open_request(socket: &mut Socket) -> String {
    let events = until(socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest { request, .. }) = events.last() else {
        panic!()
    };
    request.clone()
}

async fn respond(socket: &mut Socket, request: String, decision: ApprovalDecision) {
    send(
        socket,
        9,
        ClientCommand::ApprovalRespond { request, decision },
    )
    .await;
}

async fn answer(socket: &mut Socket, decision: ApprovalDecision) {
    let request = open_request(socket).await;
    respond(socket, request, decision).await;
}

/// A site `tools` with this justfile; returns its folder.
fn tools_site(dir: &tempfile::TempDir, justfile: &str) -> std::path::PathBuf {
    let site = dir.path().join("sites/tools");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(site.join("justfile"), justfile).unwrap();
    site
}

/// Approves the current version of a recipe for `caller`, as a user's earlier answer would have.
async fn approve_for(
    server: &composer_server::Server,
    site: &std::path::Path,
    recipe: &str,
    caller: Caller,
) {
    let hash = Justfile::load(site).await.unwrap().hash(recipe);
    server
        .approvals
        .approve("tools", recipe, &hash, caller)
        .unwrap();
}

async fn approve(server: &composer_server::Server, site: &std::path::Path, recipe: &str) {
    approve_for(server, site, recipe, Caller::Agent).await;
    approve_for(server, site, recipe, Caller::App).await;
}

#[tokio::test]
async fn routes_need_the_token() {
    let (_dir, _server, _p, addr) = start(vec![]).await;
    let client = reqwest::Client::new();
    let list = client
        .get(format!("http://{addr}/api/actions/orchestration"))
        .send()
        .await
        .unwrap();
    let run = client
        .post(format!(
            "http://{addr}/api/actions/orchestration/new-ticket"
        ))
        .json(&json!({ "args": ["x"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 401);
    assert_eq!(run.status(), 401);
}

#[tokio::test]
async fn an_app_lists_and_runs_actions_over_http() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let site = tools_site(
        &dir,
        "# Greets someone.\nhello name:\n    echo \"hi {{name}} from $COMPOSER_SITE\"\n_helper:\n    true\n",
    );

    let (status, list) = http(&server, addr, "/api/actions/tools", None).await;
    assert_eq!(status, 200);
    assert_eq!(
        list,
        json!([{ "name": "hello", "doc": "Greets someone.", "parameters": ["name"], "approved": false }])
    );
    approve(&server, &site, "hello").await;
    let (_, list) = http(&server, addr, "/api/actions/tools", None).await;
    assert_eq!(list[0]["approved"], true, "{list}");

    let (status, run) = http(
        &server,
        addr,
        "/api/actions/tools/hello",
        Some(json!({ "args": ["ann"] })),
    )
    .await;
    assert_eq!(status, 200, "{run}");
    assert_eq!(
        (run["status"].as_str(), run["exit_code"].as_i64()),
        (Some("ok"), Some(0))
    );
    assert_eq!(
        run["output"].as_str().unwrap().trim_end(),
        "echo \"hi ann from $COMPOSER_SITE\"\nhi ann from tools"
    );

    // A failing recipe is reported as failed, with its exit code.
    let site = tools_site(&dir, "boom:\n    exit 3\n");
    approve(&server, &site, "boom").await;
    let (status, run) = http(&server, addr, "/api/actions/tools/boom", Some(json!({}))).await;
    assert_eq!(status, 200, "{run}");
    assert_eq!(
        (run["status"].as_str(), run["exit_code"].as_i64()),
        (Some("failed"), Some(3))
    );

    // Private and unknown recipes are 404.
    for recipe in ["_helper", "nope"] {
        let (status, _) = http(
            &server,
            addr,
            &format!("/api/actions/tools/{recipe}"),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, 404, "{recipe}");
    }
}

#[tokio::test]
async fn a_stock_recipe_cannot_be_made_to_run_shell() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    // `new-ticket` is pre-approved and, before it read its title as "$1", pasted it into a
    // `sed` line, where this closes the quote and runs `touch injected`.
    let title = "x/g\" templates/ticket.md >/dev/null; touch injected; sed -e \"s/a/b";
    let (status, run) = http(
        &server,
        addr,
        "/api/actions/orchestration/new-ticket",
        Some(json!({ "args": [title] })),
    )
    .await;
    assert_eq!(status, 200, "{run}");
    let orchestration = dir.path().join("sites/orchestration");
    assert!(!orchestration.join("injected").exists(), "{run}");
    let ticket = std::fs::read_to_string(orchestration.join("tickets/0001.md")).unwrap();
    assert!(ticket.contains(title), "{ticket}");
}

#[tokio::test]
async fn extra_args_do_not_run_other_recipes() {
    if !has_just() {
        return;
    }
    // `just safe x unapproved` would run `unapproved` too.
    let justfile = "safe name:\n    touch ran-safe\nunapproved:\n    touch ran-unapproved\n\
                    _private:\n    touch ran-private\nbare:\n    touch ran-bare\n";
    let mut scripts = run_turn("tools", "safe", &["x", "unapproved"]);
    scripts.extend(run_turn("tools", "bare", &["_private"]));
    let (dir, server, _p, addr) = start(scripts).await;
    let site = tools_site(&dir, justfile);
    approve(&server, &site, "safe").await;
    approve(&server, &site, "bare").await;

    for (recipe, args) in [("safe", ["x", "unapproved"]), ("bare", ["_private", ""])] {
        let args: Vec<&str> = args.iter().copied().filter(|a| !a.is_empty()).collect();
        let (status, body) = http(
            &server,
            addr,
            &format!("/api/actions/tools/{recipe}"),
            Some(json!({ "args": args })),
        )
        .await;
        assert_eq!(status, 422, "{body}");
    }
    // The agent's `run` tool has the same rule.
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    for n in [3, 4] {
        ask(&mut socket, &id, n).await;
        let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
        let (result, is_error) = tool_result(&events);
        assert!(is_error && result.contains("takes"), "{result}");
    }
    let ran: Vec<_> = std::fs::read_dir(&site)
        .unwrap()
        .filter_map(|e| e.unwrap().file_name().into_string().ok())
        .filter(|n| n.starts_with("ran-"))
        .collect();
    assert!(ran.is_empty(), "{ran:?}");

    // The right number of arguments still runs.
    let (status, body) = http(
        &server,
        addr,
        "/api/actions/tools/safe",
        Some(json!({ "args": ["x"] })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(site.join("ran-safe").exists());
}

#[tokio::test]
async fn shell_syntax_in_args_is_refused_unless_the_recipe_reads_them_as_positional() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let site = tools_site(
        &dir,
        "raw name:\n    echo {{name}}\n\
         [positional-arguments]\npos name:\n    #!/usr/bin/env bash\n    echo \"got: $1\"\n",
    );
    approve(&server, &site, "raw").await;
    approve(&server, &site, "pos").await;
    let evil = "x; touch injected";
    let call = |recipe: &'static str, arg: &'static str| {
        let server = server.clone();
        async move {
            http(
                &server,
                addr,
                &format!("/api/actions/tools/{recipe}"),
                Some(json!({ "args": [arg] })),
            )
            .await
        }
    };

    let (status, body) = call("raw", evil).await;
    assert_eq!(status, 422, "{body}");
    assert!(!site.join("injected").exists());
    let (status, body) = call("raw", "plain words, 1.5").await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = call("pos", evil).await;
    assert_eq!(status, 200, "{body}");
    assert!(
        body["output"]
            .as_str()
            .unwrap()
            .contains("got: x; touch injected")
    );
    assert!(!site.join("injected").exists());
}

#[tokio::test]
async fn an_app_run_needs_approval_first() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let site = tools_site(&dir, "hello name:\n    touch \"ran-{{name}}\"\n");
    let path = "/api/actions/tools/hello";
    let ran = |name: &str| site.join(format!("ran-{name}")).exists();

    // No session to ask: refused, nothing ran.
    let (status, body) = http(&server, addr, path, Some(json!({ "args": ["none"] }))).await;
    assert_eq!(status, 403, "{body}");
    assert!(!ran("none"));

    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    let call = |name: &'static str| {
        let (server, id) = (server.clone(), id.clone());
        tokio::spawn(async move {
            http(
                &server,
                addr,
                path,
                Some(json!({ "args": [name], "session": id })),
            )
            .await
        })
    };

    // Denied: 403, nothing ran.
    let pending = call("eve");
    answer(&mut socket, ApprovalDecision::Deny).await;
    assert_eq!(pending.await.unwrap().0, 403);
    assert!(!ran("eve"));

    // Approved: runs; the next call needs no question.
    let pending = call("ann");
    answer(&mut socket, ApprovalDecision::Approve).await;
    let (status, body) = pending.await.unwrap();
    assert_eq!(status, 200, "{body}");
    assert!(ran("ann"));
    let (status, body) = http(&server, addr, path, Some(json!({ "args": ["bob"] }))).await;
    assert_eq!(status, 200, "{body}");
    assert!(ran("bob"));

    // The recipe changes: it is not approved any more.
    std::fs::write(
        site.join("justfile"),
        "hello name:\n    touch \"ran2-{{name}}\"\n",
    )
    .unwrap();
    let (status, body) = http(&server, addr, path, Some(json!({ "args": ["cy"] }))).await;
    assert_eq!(status, 403, "{body}");
    assert!(!site.join("ran2-cy").exists());

    // It changes again while the question is open: the answer is for the old version.
    let pending = call("di");
    let request = open_request(&mut socket).await;
    std::fs::write(
        site.join("justfile"),
        "hello name:\n    touch \"ran3-{{name}}\"\n",
    )
    .unwrap();
    respond(&mut socket, request, ApprovalDecision::Approve).await;
    assert_eq!(pending.await.unwrap().0, 409);
    assert!(!site.join("ran2-di").exists() && !site.join("ran3-di").exists());
}

#[tokio::test]
async fn a_client_that_goes_away_closes_its_question() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let site = tools_site(&dir, "hello:\n    touch ran\n");
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    let call = {
        let (server, id) = (server.clone(), id.clone());
        tokio::spawn(async move {
            http(
                &server,
                addr,
                "/api/actions/tools/hello",
                Some(json!({ "session": id })),
            )
            .await
        })
    };
    until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    assert_eq!(
        server.sessions.get(&id).unwrap().info().state,
        TurnState::AwaitingApproval
    );

    call.abort();
    until(&mut socket, |e| {
        matches!(
            e,
            ServerMessage::ApprovalResolved {
                approved: false,
                ..
            }
        )
    })
    .await;
    assert!(server.pending_approvals.all().is_empty());
    assert_ne!(
        server.sessions.get(&id).unwrap().info().state,
        TurnState::AwaitingApproval
    );
    assert!(!site.join("ran").exists());
}

#[tokio::test]
async fn an_agents_approval_does_not_count_on_the_route() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(run_turn("tools", "act", &[])).await;
    let site = tools_site(&dir, "act:\n    touch ran\n");
    let hash = Justfile::load(&site).await.unwrap().hash("act");
    approve_for(&server, &site, "act", Caller::Agent).await;
    let path = "/api/actions/tools/act";

    // Listed as not approved; with nobody to ask it is refused, and says why.
    let (_, list) = http(&server, addr, "/api/actions/tools", None).await;
    assert_eq!(list[0]["approved"], false, "{list}");
    let (status, _) = http(&server, addr, path, Some(json!({}))).await;
    assert_eq!(status, 403);
    assert!(!site.join("ran").exists());

    // With a session it asks, and the question says an app is asking.
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    let pending = {
        let (server, id) = (server.clone(), id.clone());
        tokio::spawn(async move {
            http(
                &server,
                addr,
                "/api/actions/tools/act",
                Some(json!({ "session": id })),
            )
            .await
        })
    };
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ApprovalRequest { .. })
    })
    .await;
    let Some(ServerMessage::ApprovalRequest { request, body, .. }) = events.last() else {
        panic!()
    };
    assert!(body.starts_with("# An app asks"), "{body}");
    respond(&mut socket, request.clone(), ApprovalDecision::Approve).await;
    assert_eq!(pending.await.unwrap().0, 200);
    assert!(site.join("ran").exists());

    // Now both hold, and the next app call needs no question.
    let approved = |caller| server.approvals.is_approved("tools", "act", &hash, caller);
    assert!(approved(Caller::App) && approved(Caller::Agent));
    let (status, _) = http(&server, addr, path, Some(json!({}))).await;
    assert_eq!(status, 200);
    let (_, list) = http(&server, addr, "/api/actions/tools", None).await;
    assert_eq!(list[0]["approved"], true, "{list}");
}

#[tokio::test]
async fn an_apps_approval_does_not_count_for_agents() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(run_turn("tools", "act", &[])).await;
    let site = tools_site(&dir, "act:\n    touch ran\n");
    approve_for(&server, &site, "act", Caller::App).await;
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    ask(&mut socket, &id, 3).await;
    answer(&mut socket, ApprovalDecision::Deny).await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(!site.join("ran").exists());
}

#[tokio::test]
async fn a_new_version_forgets_the_old_one_for_every_caller() {
    let dir = tempfile::tempdir().unwrap();
    let approvals = Approvals::load(&dir.path().join("approvals.json")).unwrap();
    approvals.approve("s", "r", "v1", Caller::Agent).unwrap();
    approvals.approve("s", "r", "v1", Caller::App).unwrap();
    assert!(approvals.is_approved("s", "r", "v1", Caller::Agent));
    assert!(approvals.is_approved("s", "r", "v1", Caller::App));

    approvals.approve("s", "r", "v2", Caller::Agent).unwrap();
    assert!(!approvals.is_approved("s", "r", "v1", Caller::Agent));
    assert!(!approvals.is_approved("s", "r", "v1", Caller::App));
    assert!(!approvals.is_approved("s", "r", "v2", Caller::App));
}

#[test]
fn saving_approvals_replaces_the_file_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approvals.json");
    let approvals = Approvals::load(&path).unwrap();
    approvals.approve("s", "r", "v1", Caller::Agent).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    // A second name for the old file: writing in place would change it too.
    let old = dir.path().join("old");
    std::fs::hard_link(&path, &old).unwrap();

    approvals.approve("s", "r", "v2", Caller::Agent).unwrap();
    assert_eq!(std::fs::read_to_string(&old).unwrap(), before);
    assert!(
        Approvals::load(&path)
            .unwrap()
            .is_approved("s", "r", "v2", Caller::Agent)
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn an_agents_prompt_for_a_marked_recipe_says_it_opens_it_to_apps() {
    if !has_just() {
        return;
    }
    let mut scripts = run_turn("tools", "act", &[]);
    scripts.extend(run_turn("tools", "plain", &[]));
    let (dir, server, _p, addr) = start(scripts).await;
    let site = tools_site(
        &dir,
        "[metadata('app')]\nact:\n    touch ran\nplain:\n    touch plain\n",
    );
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    let prompt = async |socket: &mut Socket| {
        let events = until(socket, |e| {
            matches!(e, ServerMessage::ApprovalRequest { .. })
        })
        .await;
        let Some(ServerMessage::ApprovalRequest { request, body, .. }) = events.last() else {
            panic!()
        };
        (request.clone(), body.clone())
    };

    ask(&mut socket, &id, 3).await;
    let (request, body) = prompt(&mut socket).await;
    assert!(
        body.starts_with("# Marked [metadata('app')]. Approving also lets any app"),
        "{body}"
    );
    respond(&mut socket, request, ApprovalDecision::Approve).await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(site.join("ran").exists());
    // What it warned about: apps now run it unasked.
    std::fs::remove_file(site.join("ran")).unwrap();
    let (status, _) = http(&server, addr, "/api/actions/tools/act", Some(json!({}))).await;
    assert_eq!(status, 200);
    assert!(site.join("ran").exists());

    // A recipe without the mark opens nothing, and doesn't say so.
    ask(&mut socket, &id, 4).await;
    let (request, body) = prompt(&mut socket).await;
    assert!(!body.contains("any app"), "{body}");
    respond(&mut socket, request, ApprovalDecision::Deny).await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
}

#[tokio::test]
async fn the_app_mark_extends_an_approval_but_is_not_one() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let plain = "act:\n    touch ran\n";
    let marked = "[metadata('app')]\nact:\n    touch ran\n";
    let site = tools_site(&dir, plain);
    approve_for(&server, &site, "act", Caller::Agent).await;
    let path = "/api/actions/tools/act";

    // The token can write justfiles: marking a recipe through it must not make it runnable,
    // nor may it keep the approval of the version without the mark.
    let written = reqwest::Client::new()
        .put(format!("http://{addr}/api/files/tools/justfile"))
        .bearer_auth(&server.token)
        .body(marked)
        .send()
        .await
        .unwrap();
    assert_eq!(written.status(), 200);
    let (status, _) = http(&server, addr, path, Some(json!({}))).await;
    assert_eq!(status, 403);
    assert!(!site.join("ran").exists());

    // Once the user approved the marked version for agents, apps may run it unasked,
    // and the list says so.
    approve_for(&server, &site, "act", Caller::Agent).await;
    let (_, list) = http(&server, addr, "/api/actions/tools", None).await;
    assert_eq!(list[0]["approved"], true, "{list}");
    let (status, body) = http(&server, addr, path, Some(json!({}))).await;
    assert_eq!(status, 200, "{body}");
    assert!(site.join("ran").exists());

    // A marked version shows its mark in the question the user answers.
    let body = Justfile::load(&site)
        .await
        .unwrap()
        .show("act")
        .await
        .unwrap();
    assert!(body.contains("[metadata('app')]"), "{body}");

    // Any other metadata is not the mark.
    let site = tools_site(&dir, "[metadata('other')]\nact:\n    touch ran\n");
    approve_for(&server, &site, "act", Caller::Agent).await;
    let (status, _) = http(&server, addr, path, Some(json!({}))).await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn stock_recipes_stay_preapproved_for_apps_also_on_a_home_from_before_callers() {
    if !has_just() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let approved = |site: &'static str, recipe: &'static str, caller| {
        let approvals = Approvals::load(&home.approvals_file()).unwrap();
        let dir = home.root().join("sites").join(site);
        async move {
            let hash = Justfile::load(&dir).await.unwrap().hash(recipe);
            approvals.is_approved(site, recipe, &hash, caller)
        }
    };
    for (site, recipe) in [("orchestration", "new-ticket"), ("chat", "ui-build")] {
        assert!(approved(site, recipe, Caller::Agent).await, "{recipe}");
        assert!(approved(site, recipe, Caller::App).await, "{recipe}");
    }

    // approvals.json as it was: no `caller`. Entries stay valid for agents, and no more.
    let path = home.approvals_file();
    let mut old: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    old.retain(|a| a["caller"] == "agent");
    for approval in &mut old {
        approval.as_object_mut().unwrap().remove("caller");
    }
    std::fs::write(&path, serde_json::to_string(&old).unwrap()).unwrap();
    assert!(approved("chat", "ui-build", Caller::Agent).await);
    assert!(!approved("chat", "ui-build", Caller::App).await);

    // `update` catches up the stock justfiles, even ones with nothing new to install.
    update_home(&home, false).await.unwrap();
    assert!(approved("chat", "ui-build", Caller::App).await);
    assert!(approved("orchestration", "new-ticket", Caller::App).await);

    // A justfile the user changed is theirs: it stays as it was.
    let chat = home.root().join("sites/chat/justfile");
    let edited = std::fs::read_to_string(&chat).unwrap() + "\nextra:\n    true\n";
    std::fs::write(&chat, edited).unwrap();
    let hash = Justfile::load(chat.parent().unwrap())
        .await
        .unwrap()
        .hash("extra");
    update_home(&home, false).await.unwrap();
    let approvals = Approvals::load(&home.approvals_file()).unwrap();
    assert!(!approvals.is_approved("chat", "extra", &hash, Caller::App));
}

/// A ticket's session under an orchestrator: `[system1]` or its orchestrator decide its approvals.
fn worker_info(parent: &str, state: TurnState) -> SessionInfo {
    SessionInfo {
        id: "w1".into(),
        agent: "worker".into(),
        project: None,
        ticket: Some("0001".into()),
        parent: Some(parent.into()),
        worker: None,
        state,
        last_seq: 0,
        context_tokens: 0,
        model: None,
        preview: None,
        updated: String::new(),
    }
}

type Fake = (Arc<Mutex<String>>, Duration);

/// A `[system1]` endpoint that streams `answer` after `delay`.
async fn fake_system1(answer: Arc<Mutex<String>>, delay: Duration) -> String {
    async fn complete(State((answer, delay)): State<Fake>, _body: axum::Json<Value>) -> String {
        tokio::time::sleep(delay).await;
        let chunk = json!({ "choices": [{ "delta": { "content": *answer.lock().unwrap() }, "finish_reason": "stop" }] });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }
    let app = Router::new()
        .route("/v1/chat/completions", post(complete))
        .with_state((answer, delay));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}/v1")
}

async fn system1_config(answer: Arc<Mutex<String>>, delay: Duration) -> Config {
    let mut config = Config::defaults();
    config.system1.model = "tiny".into();
    config.system1.base_url = fake_system1(answer, delay).await;
    config
}

#[tokio::test]
async fn a_delegated_answer_holds_for_one_run_and_a_users_is_remembered() {
    if !has_just() {
        return;
    }
    let verdict = Arc::new(Mutex::new(
        r#"{"decision":"approve","reason":"ok"}"#.to_string(),
    ));
    let config = system1_config(verdict.clone(), Duration::ZERO).await;
    let mut scripts = run_turn("tools", "act", &[]);
    scripts.extend(run_turn("tools", "act", &[]));
    scripts.extend(run_turn("tools", "act", &[]));
    let (dir, server, _p, addr) = start_with(config, scripts).await;
    let site = tools_site(&dir, "[metadata('app')]\nact:\n    touch ran\n");
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    server
        .sessions
        .create_with_info(worker_info(orchestrator.id(), TurnState::Idle))
        .unwrap();
    let hash = Justfile::load(&site).await.unwrap().hash("act");
    let path = "/api/actions/tools/act";
    let remembered = || {
        server
            .approvals
            .is_approved("tools", "act", &hash, Caller::Agent)
    };
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: "w1".into(),
            since: None,
        },
    )
    .await;

    // `[system1]` approves: it runs, and nothing is remembered, so the mark opens nothing.
    ask(&mut socket, "w1", 3).await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(site.join("ran").exists());
    assert!(!remembered());
    assert_eq!(http(&server, addr, path, Some(json!({}))).await.0, 403);

    // `[system1]` denies: it doesn't run.
    std::fs::remove_file(site.join("ran")).unwrap();
    *verdict.lock().unwrap() = r#"{"decision":"deny","reason":"no"}"#.into();
    ask(&mut socket, "w1", 4).await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(!site.join("ran").exists());

    // `[system1]` has no view: the user answers, and that is remembered.
    *verdict.lock().unwrap() = "hmm".into();
    ask(&mut socket, "w1", 5).await;
    answer(&mut socket, ApprovalDecision::Approve).await;
    until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(site.join("ran").exists());
    assert!(remembered());
}

/// An app's call naming a ticket's session, sent while nobody has answered.
fn app_call_in_w1(
    server: &Arc<Server>,
    addr: std::net::SocketAddr,
) -> tokio::task::JoinHandle<(u16, Value)> {
    let server = server.clone();
    tokio::spawn(async move {
        http(
            &server,
            addr,
            "/api/actions/tools/act",
            Some(json!({ "session": "w1" })),
        )
        .await
    })
}

#[tokio::test]
async fn an_apps_call_in_a_tickets_session_asks_the_user_not_system1() {
    if !has_just() {
        return;
    }
    let verdict = Arc::new(Mutex::new(
        r#"{"decision":"approve","reason":"ok"}"#.to_string(),
    ));
    let config = system1_config(verdict, Duration::ZERO).await;
    let (dir, server, _p, addr) = start_with(config, vec![]).await;
    let site = tools_site(&dir, "act:\n    touch ran\n");
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    server
        .sessions
        .create_with_info(worker_info(orchestrator.id(), TurnState::Idle))
        .unwrap();
    let hash = Justfile::load(&site).await.unwrap().hash("act");
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: "w1".into(),
            since: None,
        },
    )
    .await;

    let pending = app_call_in_w1(&server, addr);
    let request = open_request(&mut socket).await;
    assert!(!pending.is_finished());
    assert!(!site.join("ran").exists());
    respond(&mut socket, request, ApprovalDecision::Approve).await;
    assert_eq!(pending.await.unwrap().0, 200);
    assert!(site.join("ran").exists());
    assert!(
        server
            .approvals
            .is_approved("tools", "act", &hash, Caller::App)
    );
}

#[tokio::test]
async fn an_apps_call_in_a_tickets_session_asks_the_user_not_its_orchestrator() {
    if !has_just() {
        return;
    }
    let (dir, server, provider, addr) = start(vec![]).await;
    provider.stalled.store(true, Ordering::SeqCst);
    let site = tools_site(&dir, "act:\n    touch ran\n");
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    server
        .sessions
        .create_with_info(worker_info(orchestrator.id(), TurnState::Idle))
        .unwrap();
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: "w1".into(),
            since: None,
        },
    )
    .await;

    let pending = app_call_in_w1(&server, addr);
    let request = open_request(&mut socket).await;
    assert!(provider.requests().is_empty(), "the orchestrator was asked");
    respond(&mut socket, request, ApprovalDecision::Deny).await;
    assert_eq!(pending.await.unwrap().0, 403);
    assert!(!site.join("ran").exists());
}

/// Polls until `done`, for what happens a moment after a client drops its request.
async fn eventually(done: impl Fn() -> bool) {
    for _ in 0..200 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// An agent's call reaching its approval as a `run` would; aborting it is the turn going away.
async fn agent_call(
    server: &Arc<Server>,
    worker: &Arc<Session>,
    site: &std::path::Path,
) -> tokio::task::JoinHandle<()> {
    let justfile = Justfile::load(site).await.unwrap();
    let recipe = justfile.recipe("act").unwrap();
    let (server, worker) = (server.clone(), worker.clone());
    tokio::spawn(async move {
        let cancel = tokio_util::sync::CancellationToken::new();
        let _ = ensure_approved(
            &server,
            &worker,
            "tools",
            &justfile,
            &recipe,
            &[],
            Caller::Agent,
            &cancel,
        )
        .await;
    })
}

#[tokio::test]
async fn a_caller_that_goes_away_while_system1_decides_closes_the_request() {
    if !has_just() {
        return;
    }
    let verdict = Arc::new(Mutex::new(
        r#"{"decision":"approve","reason":"ok"}"#.to_string(),
    ));
    let config = system1_config(verdict, Duration::from_secs(60)).await;
    let (dir, server, _p, _addr) = start_with(config, vec![]).await;
    let site = tools_site(&dir, "act:\n    touch ran\n");
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    let worker = server
        .sessions
        .create_with_info(worker_info(orchestrator.id(), TurnState::Idle))
        .unwrap();
    let call = agent_call(&server, &worker, &site).await;
    let waiting = || worker.info().state == TurnState::AwaitingApproval;
    eventually(waiting).await;
    assert!(waiting());

    call.abort();
    eventually(|| !waiting()).await;
    assert_eq!(worker.info().state, TurnState::Idle);
    assert!(server.pending_approvals.all().is_empty());
    assert!(!site.join("ran").exists());
}

#[tokio::test]
async fn a_caller_that_goes_away_while_the_orchestrator_decides_closes_the_request() {
    if !has_just() {
        return;
    }
    let (dir, server, provider, _addr) = start(vec![]).await;
    // Its turn stays in progress, so the request stays with it.
    provider.stalled.store(true, Ordering::SeqCst);
    let site = tools_site(&dir, "act:\n    touch ran\n");
    let orchestrator = server.sessions.create("orchestrator", None).unwrap();
    let worker = server
        .sessions
        .create_with_info(worker_info(orchestrator.id(), TurnState::Idle))
        .unwrap();
    let call = agent_call(&server, &worker, &site).await;
    let asked = || !provider.requests().is_empty();
    eventually(asked).await;
    assert!(asked());
    assert_eq!(worker.info().state, TurnState::AwaitingApproval);

    call.abort();
    eventually(|| worker.info().state != TurnState::AwaitingApproval).await;
    assert_eq!(worker.info().state, TurnState::Idle);
    let answered = server.worker_approvals.answer(
        orchestrator.id(),
        "0001",
        None,
        Verdict::Approve("too late".into()),
    );
    assert!(answered.unwrap_err().contains("no approval"));
    assert!(!site.join("ran").exists());
}

async fn two_prompts_answered(first_answered_first: bool) {
    let (dir, server, _p, addr) = start(vec![]).await;
    tools_site(&dir, "one:\n    touch one\ntwo:\n    touch two\n");
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    let call = |recipe: &'static str| {
        let (server, id) = (server.clone(), id.clone());
        tokio::spawn(async move {
            http(
                &server,
                addr,
                &format!("/api/actions/tools/{recipe}"),
                Some(json!({ "session": id })),
            )
            .await
        })
    };
    let state = || server.sessions.get(&id).unwrap().info().state;
    let one = call("one");
    let first = open_request(&mut socket).await;
    let two = call("two");
    let second = open_request(&mut socket).await;
    let (a, b, done) = if first_answered_first {
        (first, second, one)
    } else {
        (second, first, two)
    };
    respond(&mut socket, a, ApprovalDecision::Approve).await;
    assert_eq!(done.await.unwrap().0, 200);
    assert_eq!(state(), TurnState::AwaitingApproval);
    respond(&mut socket, b, ApprovalDecision::Approve).await;
    eventually(|| state() != TurnState::AwaitingApproval).await;
    assert_eq!(state(), TurnState::Idle);
}

#[tokio::test]
async fn a_session_shows_needs_approval_until_its_last_prompt_closes() {
    if !has_just() {
        return;
    }
    two_prompts_answered(true).await;
    two_prompts_answered(false).await;
}

#[tokio::test]
async fn a_prompt_that_outlives_a_turn_leaves_the_turns_state_alone() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    tools_site(&dir, "act:\n    true\n");
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    let target = server.sessions.get(&id).unwrap();

    // The state it was opened in, and the one a turn moved it to: a turn ended, a turn started.
    for (opened, moved) in [
        (TurnState::Running, TurnState::Idle),
        (TurnState::Idle, TurnState::Running),
    ] {
        target.update_info(&server.hub, |info| info.state = opened);
        let pending = {
            let (server, id) = (server.clone(), id.clone());
            tokio::spawn(async move {
                http(
                    &server,
                    addr,
                    "/api/actions/tools/act",
                    Some(json!({ "session": id })),
                )
                .await
            })
        };
        let request = open_request(&mut socket).await;
        target.update_info(&server.hub, |info| info.state = moved);
        respond(&mut socket, request, ApprovalDecision::Deny).await;
        assert_eq!(pending.await.unwrap().0, 403);
        assert_eq!(target.info().state, moved);
    }
}

#[tokio::test]
async fn recipes_on_the_route_get_their_composer_variables() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let site = tools_site(
        &dir,
        "env:\n    @echo \"$COMPOSER_SITE|$COMPOSER_SESSION|$COMPOSER_PROJECT\"\n",
    );
    approve(&server, &site, "env").await;
    let session = server
        .sessions
        .create("default", Some("/some/project".to_string()))
        .unwrap();
    let (status, body) = http(
        &server,
        addr,
        "/api/actions/tools/env",
        Some(json!({ "session": session.id() })),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["output"].as_str().unwrap().trim(),
        format!("tools|{}|/some/project", session.id())
    );
}

#[tokio::test]
async fn variadic_recipes_take_any_number_of_arguments_and_no_other_recipes_run() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
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
    let call = |recipe: &'static str, args: &'static [&'static str]| {
        let server = server.clone();
        async move {
            http(
                &server,
                addr,
                &format!("/api/actions/tools/{recipe}"),
                Some(json!({ "args": args })),
            )
            .await
            .0
        }
    };
    let count = |recipe: &str| {
        std::fs::read_to_string(site.join(format!("{recipe}.out")))
            .unwrap()
            .trim()
            .to_string()
    };

    assert_eq!(call("star", &["a", "b", "other"]).await, 200);
    assert_eq!(count("star"), "3");
    assert_eq!(call("plus", &["a", "b"]).await, 200);
    assert_eq!(count("plus"), "2");
    assert_eq!(call("mix", &["a", "b", "c"]).await, 200);
    assert_eq!(count("mix"), "3");
    assert_eq!(call("dflt", &["a"]).await, 200);
    assert_eq!(call("dflt", &["a", "b"]).await, 200);
    // Not variadic: the argument too many would be a recipe to `just`.
    assert_eq!(call("dflt", &["a", "b", "other"]).await, 422);
    assert_eq!(call("none", &["other"]).await, 422);
    assert!(!site.join("none.out").exists());
    assert!(
        !site.join("other.out").exists(),
        "another recipe ran on the way"
    );
}

#[tokio::test]
async fn every_way_of_pasting_a_parameter_is_read_as_pasting() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let site = tools_site(
        &dir,
        "dep x:\n    echo {{x}}\n\
         raw n:\n    echo {{n}}\n\
         spaced n:\n    echo {{ n }}\n\
         concat n:\n    echo {{ 'a' + n }}\n\
         prior n: (dep n)\n\
         after n: && (dep n)\n    true\n\
         var *n:\n    echo {{n}}\n\
         shebang n:\n    #!/usr/bin/env bash\n    echo {{n}}\n\
         quoted n:\n    echo {{ quote(n) }}\n",
    );
    for recipe in [
        "raw", "spaced", "concat", "prior", "after", "var", "shebang", "quoted",
    ] {
        approve(&server, &site, recipe).await;
        let (status, body) = http(
            &server,
            addr,
            &format!("/api/actions/tools/{recipe}"),
            Some(json!({ "args": ["x; touch injected"] })),
        )
        .await;
        assert_eq!(status, 422, "{recipe}: {body}");
        assert!(!site.join("injected").exists(), "{recipe}");
    }
}

#[tokio::test]
async fn the_refusal_of_a_pasted_argument_names_the_quote_form() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let site = tools_site(&dir, "quoted n:\n    echo {{ quote(n) }}\n");
    approve(&server, &site, "quoted").await;
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/api/actions/tools/quoted"))
        .bearer_auth(&server.token)
        .json(&json!({ "args": ["a; b"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    let error = response.text().await.unwrap();
    assert!(error.contains("quote(name)"), "{error}");
}

#[tokio::test]
async fn options_are_refused_on_the_route_but_not_for_agents() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(run_turn("tools", "pos", &["--version"])).await;
    let site = tools_site(
        &dir,
        "[positional-arguments]\npos f:\n    echo \"got: $1\" > pos.out\n\
         find n:\n    find data -name {{n}}\n",
    );
    std::fs::create_dir_all(site.join("data")).unwrap();
    std::fs::write(site.join("data/keep"), "x").unwrap();
    approve(&server, &site, "pos").await;
    approve(&server, &site, "find").await;
    let call = |recipe: &'static str, arg: &'static str| {
        let server = server.clone();
        async move {
            http(
                &server,
                addr,
                &format!("/api/actions/tools/{recipe}"),
                Some(json!({ "args": [arg] })),
            )
            .await
            .0
        }
    };

    // Starting with `-`, and a word of a pasted argument starting with `-`.
    assert_eq!(call("pos", "--version").await, 422);
    assert_eq!(call("pos", "-").await, 422);
    assert_eq!(call("find", "nomatch -o -delete").await, 422);
    assert!(!site.join("pos.out").exists());
    assert!(site.join("data/keep").exists());

    // Read as "$1" it stays one argument, so a `-` further on is just text; a word's inner `-` too.
    assert_eq!(call("pos", "fix the -x flag").await, 200);
    assert_eq!(call("find", "well-known").await, 200);

    // An agent's call isn't limited this way.
    let mut socket = connect(addr, &server.token).await;
    let id = session(&mut socket).await;
    ask(&mut socket, &id, 3).await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let (result, is_error) = tool_result(&events);
    assert!(!is_error, "{result}");
    let out = std::fs::read_to_string(site.join("pos.out")).unwrap();
    assert_eq!(out.trim(), "got: --version");
}

#[tokio::test]
async fn a_title_with_a_newline_uses_no_ticket_id() {
    if !has_just() {
        return;
    }
    let (dir, server, _p, addr) = start(vec![]).await;
    let tickets = dir.path().join("sites/orchestration/tickets");
    let path = "/api/actions/orchestration/new-ticket";
    let (status, run) = http(&server, addr, path, Some(json!({ "args": ["two\nlines"] }))).await;
    assert_eq!(status, 200, "{run}");
    assert_eq!(run["status"], "failed", "{run}");
    assert!(
        run["output"].as_str().unwrap().contains("one line"),
        "{run}"
    );
    assert!(!tickets.join("0001.md").exists());

    let (_, run) = http(&server, addr, path, Some(json!({ "args": ["one line"] }))).await;
    assert_eq!(run["status"], "ok", "{run}");
    assert!(tickets.join("0001.md").exists());
}
