//! `config.toml` is read again when it changes (and on `config.reload`): a project allowed an
//! external harness after the server started can use it, a file that doesn't parse keeps the
//! running config, and settings read only at startup are named as needing a restart.

mod support;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use support::*;

async fn command(socket: &mut Socket, id: u64, command: ClientCommand) -> CommandResult {
    send(socket, id, command).await;
    let reply = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    match reply.last() {
        Some(ServerMessage::Ok { result, .. }) => result.clone(),
        other => panic!("{other:?}"),
    }
}

async fn claude_code_allowed(socket: &mut Socket, id: u64, project: &str) -> bool {
    let CommandResult::Harnesses { harnesses, .. } = command(
        socket,
        id,
        ClientCommand::HarnessList {
            project: project.into(),
        },
    )
    .await
    else {
        panic!()
    };
    harnesses
        .iter()
        .any(|h| h.entry == "claude-code" && h.allowed)
}

/// The next `config.reloaded` broadcast: its `restart_needed` and `error`.
async fn reloaded(socket: &mut Socket) -> (Vec<String>, Option<String>) {
    let events = until(socket, |e| matches!(e, ServerMessage::ConfigReloaded { .. })).await;
    match events.last() {
        Some(ServerMessage::ConfigReloaded {
            restart_needed,
            error,
        }) => (restart_needed.clone(), error.clone()),
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn a_changed_config_applies_without_a_restart() {
    let (dir, server, _provider, addr) = start(vec![]).await;
    let project = dir.path().join("brain");
    std::fs::create_dir_all(&project).unwrap();
    let project = project.display().to_string();
    let config_file = server.home.config_file();
    let mut socket = connect(addr, &server.token).await;

    assert!(!claude_code_allowed(&mut socket, 1, &project).await);

    // Saved while the server runs: the watcher reloads it and tells every client.
    let allowed = format!("[projects.\"{project}\"]\nexternal_harnesses = [\"claude-code\"]\n");
    std::fs::write(&config_file, &allowed).unwrap();
    assert_eq!(reloaded(&mut socket).await, (vec![], None));
    assert!(claude_code_allowed(&mut socket, 2, &project).await);

    // Asking again with nothing changed says so.
    assert_eq!(
        command(&mut socket, 3, ClientCommand::ConfigReload).await,
        CommandResult::ConfigReloaded {
            changed: false,
            restart_needed: vec![],
        }
    );

    // A file that doesn't parse: the error is told, the running config stays.
    std::fs::write(&config_file, "[projects\nbroken").unwrap();
    let (restart_needed, error) = reloaded(&mut socket).await;
    assert!(restart_needed.is_empty());
    assert!(error.is_some_and(|e| e.contains("line 1")));
    assert!(claude_code_allowed(&mut socket, 4, &project).await);

    // Settings read at startup apply after a restart, and are named; the rest applies now.
    std::fs::write(
        &config_file,
        format!(
            "{allowed}\n[provider]\nbase_url = \"http://127.0.0.1:1/v1\"\n\n[mcp.docs]\nurl = \"http://127.0.0.1:1/mcp\"\nenabled = false\n"
        ),
    )
    .unwrap();
    assert_eq!(reloaded(&mut socket).await, (vec!["provider".into()], None));
    let CommandResult::McpServers { servers } = command(&mut socket, 5, ClientCommand::McpList).await
    else {
        panic!()
    };
    assert_eq!(servers.len(), 1);
    assert_eq!((servers[0].name.as_str(), servers[0].state.as_str()), ("docs", "off"));

    // Removed again, from `config.reload` (the watcher's broadcast may come first or not at all
    // when it already reloaded: either way the server runs without it).
    std::fs::write(&config_file, &allowed).unwrap();
    let _ = command(&mut socket, 6, ClientCommand::ConfigReload).await;
    let CommandResult::McpServers { servers } = command(&mut socket, 7, ClientCommand::McpList).await
    else {
        panic!()
    };
    assert!(servers.is_empty());
    assert!(server.config().provider.base_url != "http://127.0.0.1:1/v1");
}
