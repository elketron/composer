//! Site actions: listing, running, and the approve-once-per-version rule. Needs `just`.

mod support;

use composer_protocol::{ApprovalDecision, ClientCommand, CommandResult, ServerMessage, TurnState};
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

/// Approves the current version of a recipe, as a user's earlier answer would have.
async fn approve(server: &composer_server::Server, site: &std::path::Path, recipe: &str) {
    let hash = Justfile::load(site).await.unwrap().hash(recipe);
    server.approvals.approve("tools", recipe, &hash).unwrap();
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
