//! Background processes (the idea `background-processes`): the agent starts a command in a
//! terminal of its own and gets its id at once; approved like `terminal_send`; its exit puts a
//! message into the session; stopping it ends its whole process group; so does the server
//! stopping.

mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use composer_protocol::{
    ApprovalDecision, ChatMessage, ClientCommand, CommandResult, MessageSource, Role, RunKind,
    ServerMessage,
};
use composer_server::Config;
use composer_server::tools::ToolContext;
use serde_json::json;
use support::*;
use tokio_util::sync::CancellationToken;

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Waits (10 s) for `pid` to be gone.
async fn gone(pid: i32) -> bool {
    for _ in 0..100 {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// The pid a script wrote to `file` (10 s).
async fn pid_in(file: &Path) -> i32 {
    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(file)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no pid in {}", file.display())
}

/// The next message from a background process in the session (`stop` events after other ones).
async fn process_message(socket: &mut Socket) -> ChatMessage {
    loop {
        if let ServerMessage::MessageDone { message, .. } = next_event(socket).await
            && matches!(message.source, Some(MessageSource::Process { .. }))
        {
            return message;
        }
    }
}

#[tokio::test]
async fn background_processes() {
    // SAFETY: before the server starts; the only test in this binary.
    unsafe { std::env::set_var("SHELL", "/bin/sh") };
    let (dir, server, _provider, addr) = start_with(Config::defaults(), vec![]).await;
    let scripts = dir.path().join("scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let script = |name: &str, body: String| {
        let path = scripts.join(name);
        std::fs::write(&path, body).unwrap();
        format!("sh {}", path.display())
    };
    let session = server
        .sessions
        .create(RunKind::Agentic, None, None)
        .unwrap();
    let sid = session.id().to_string();
    let mut client = connect(addr, &server.token).await;
    send(
        &mut client,
        1,
        ClientCommand::SessionAttach {
            session: sid.clone(),
            since: None,
        },
    )
    .await;
    until(&mut client, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let context = ToolContext {
        server: server.clone(),
        run: server.sink(&session),
        kind: RunKind::Agentic,
        only: None,
        site: None,
        cancel: CancellationToken::new(),
    };
    let call = |name: &'static str, args: serde_json::Value| {
        let (server, context) = (server.clone(), context.clone());
        async move { server.tools.call_value(&context, name, args).await }
    };
    let started_id = |text: &str| {
        text.split("as terminal ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no id in: {text}"))
            .to_string()
    };

    // Asked first, like `terminal_send`: denied, nothing runs.
    let marker = dir.path().join("ran");
    let denied = tokio::spawn(call(
        "process_start",
        json!({ "command": script("touch.sh", format!("touch {}\n", marker.display())) }),
    ));
    let request = loop {
        if let ServerMessage::ApprovalRequest {
            request,
            title,
            rule,
            ..
        } = next_event(&mut client).await
        {
            assert!(title.contains("background"), "{title}");
            assert_eq!(
                rule.as_deref(),
                Some(format!("Bash(sh {}:*)", scripts.join("touch.sh").display()).as_str())
            );
            break request;
        }
    };
    send(
        &mut client,
        2,
        ClientCommand::ApprovalRespond {
            request,
            decision: ApprovalDecision::Deny,
        },
    )
    .await;
    let result = denied.await.unwrap();
    assert!(
        result.is_error && result.text.contains("didn't let you"),
        "{}",
        result.text
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!marker.exists());
    assert!(server.terminals.processes(&sid).is_empty());

    // A rule the user said `always` to (this folder: none, so `home`) covers it: no question.
    server
        .tool_rules
        .add("terminal:home", "Bash(sh:*)")
        .unwrap();

    // Returns at once; the exit (code, last lines) is posted into the session.
    let at = Instant::now();
    let started = call(
        "process_start",
        json!({ "command": script("exits.sh", "sleep 1\necho hello-from-the-background\nexit 3\n".into()) }),
    )
    .await;
    assert!(!started.is_error, "{}", started.text);
    assert!(
        at.elapsed() < Duration::from_millis(900),
        "{:?}",
        at.elapsed()
    );
    let id = started_id(&started.text);
    let listed = server.terminals.processes(&sid);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].terminal, id);
    assert!(listed[0].command.ends_with("exits.sh"));
    // On the desk, a tab the agent opened and nobody has looked at.
    let desk = session.desk();
    let tab = desk
        .tabs
        .iter()
        .find(|t| t.id == format!("terminal:{id}"))
        .expect("a desk tab");
    assert!(tab.unseen);
    let list = call("process_list", json!({})).await;
    assert!(list.text.contains(&id), "{}", list.text);
    let message = process_message(&mut client).await;
    assert_eq!(message.role, Role::User);
    assert!(
        message.content.contains(&id)
            && message.content.contains("exited with code 3")
            && message.content.contains("hello-from-the-background"),
        "{}",
        message.content
    );
    assert!(server.terminals.processes(&sid).is_empty());
    assert!(server.terminals.get(&id).is_none());

    // Stopped by the agent: its whole group goes, quietly (the tool result says it).
    let pid_file = dir.path().join("child.pid");
    let started = call(
        "process_start",
        json!({ "command": script("group.sh", format!("sleep 60 &\necho $! > {}\nsleep 60\n", pid_file.display())) }),
    )
    .await;
    let id = started_id(&started.text);
    let child = pid_in(&pid_file).await;
    let leader = server.terminals.get(&id).unwrap().pid().unwrap() as i32;
    let stopped = call("process_stop", json!({ "id": id })).await;
    assert!(!stopped.is_error, "{}", stopped.text);
    assert!(
        gone(child).await,
        "the group's child {child} outlived the stop"
    );
    assert!(gone(leader).await);
    for _ in 0..50 {
        if server.terminals.get(&id).is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(server.terminals.get(&id).is_none());

    // Stopped by the user (a client): the agent is told.
    let started = call(
        "process_start",
        json!({ "command": script("waits.sh", "echo waiting\nsleep 60\n".into()) }),
    )
    .await;
    let id = started_id(&started.text);
    let CommandResult::Processes { processes } = ({
        send(
            &mut client,
            3,
            ClientCommand::ProcessList {
                session: sid.clone(),
            },
        )
        .await;
        match until(&mut client, |e| {
            matches!(e, ServerMessage::Ok { id: 3, .. })
        })
        .await
        .pop()
        {
            Some(ServerMessage::Ok { result, .. }) => result,
            other => panic!("{other:?}"),
        }
    }) else {
        panic!()
    };
    assert_eq!(processes.len(), 1);
    send(
        &mut client,
        4,
        ClientCommand::ProcessStop {
            terminal: id.clone(),
        },
    )
    .await;
    let message = process_message(&mut client).await;
    assert!(
        message.content.contains(&id) && message.content.contains("stopped by the user"),
        "{}",
        message.content
    );
    // The user knows they stopped it: the note waits for the next turn instead of starting one.
    let woke = tokio::time::timeout(Duration::from_millis(1500), async {
        loop {
            if let ServerMessage::TurnStarted { .. } = next_event(&mut client).await {
                return;
            }
        }
    })
    .await;
    assert!(woke.is_err(), "a user stop started a turn");

    // The server stopping ends every one, and what they started.
    let pid_file = dir.path().join("left.pid");
    call(
        "process_start",
        json!({ "command": script("leaves.sh", format!("sleep 60 &\necho $! > {}\nwait\n", pid_file.display())) }),
    )
    .await;
    let child = pid_in(&pid_file).await;
    let ids: Vec<String> = server
        .terminals
        .processes(&sid)
        .into_iter()
        .map(|p| p.terminal)
        .collect();
    let leaders: Vec<i32> = ids
        .iter()
        .map(|id| server.terminals.get(id).unwrap().pid().unwrap() as i32)
        .collect();
    let stopping = server.clone();
    tokio::task::spawn_blocking(move || stopping.terminals.shutdown())
        .await
        .unwrap();
    assert!(gone(child).await, "{child} outlived the server");
    for leader in leaders {
        assert!(gone(leader).await, "{leader} outlived the server");
    }
}
