//! Reminders going off: every client hears it once, the page records it, and `action: agent`
//! hands it to a session.

mod support;

use chrono::NaiveDateTime;
use composer_core::SiteUrl;
use composer_protocol::{ChangedBy, ClientCommand, MessageSource, ServerMessage};
use composer_server::provider::ModelEvent;
use composer_server::reminders::check_reminders;
use support::*;

fn at(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
}

#[tokio::test]
async fn a_due_reminder_is_delivered_once() {
    let (dir, server, _p, addr) = start(vec![vec![
        ModelEvent::Content("Time to call.".into()),
        finished(vec![]),
    ]])
    .await;
    let url = SiteUrl::new("plans", "reminders/call.md").unwrap();
    server
        .write_page(
            &url,
            "---\ntitle: Call the printer shop\ntype: reminder\ndue: 2099-10-19\ntime: \"20:00\"\nstatus: open\n---\n# Call\n",
            ChangedBy::Agent,
        )
        .unwrap();
    let mut socket = connect(addr, &server.token).await;

    assert!(check_reminders(&server, at("2099-10-19 19:59")).is_empty());
    let delivered = check_reminders(&server, at("2099-10-19 20:00"));
    assert_eq!(delivered.len(), 1);
    let event = until(&mut socket, |e| {
        matches!(e, ServerMessage::ReminderDue { .. })
    })
    .await;
    // No session of its own: a new one for the default agent.
    let ServerMessage::ReminderDue {
        url,
        title,
        due,
        late: false,
        session: Some(session),
    } = event.last().unwrap()
    else {
        panic!("{event:?}")
    };
    assert_eq!(
        (url.as_str(), title.as_str(), due.as_str()),
        (
            "site://plans/reminders/call.md",
            "Call the printer shop",
            "2099-10-19 20:00"
        )
    );
    assert!(event.iter().any(
        |e| matches!(e, ServerMessage::SessionUpdated { session: s } if &s.id == session && s.agent == "default")
    ));
    let page = std::fs::read_to_string(dir.path().join("sites/plans/reminders/call.md")).unwrap();
    assert!(
        page.contains("status: open\nnotified: 2099-10-19 20:00\n---"),
        "{page}"
    );

    // Once: the next checks find nothing.
    assert!(check_reminders(&server, at("2099-10-19 20:01")).is_empty());
    assert!(check_reminders(&server, at("2099-10-25 09:00")).is_empty());
}

#[tokio::test]
async fn an_agent_reminder_starts_a_turn() {
    let (_dir, server, _p, addr) = start(vec![vec![
        ModelEvent::Content("On it.".into()),
        finished(vec![]),
    ]])
    .await;
    let url = SiteUrl::new("plans", "reminders/review.md").unwrap();
    server
        .write_page(
            &url,
            "---\ntitle: Review the tickets\ntype: reminder\ndue: 2099-10-19 08:00\nagent: default\n---\n",
            ChangedBy::Agent,
        )
        .unwrap();
    let mut socket = connect(addr, &server.token).await;

    // Delivered the next morning: late, in a new session for the default agent.
    check_reminders(&server, at("2099-10-20 07:00"));
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ReminderDue { .. })
    })
    .await;
    let ServerMessage::ReminderDue {
        late: true,
        session: Some(session),
        ..
    } = events.last().unwrap()
    else {
        panic!("{events:?}")
    };
    assert!(events.iter().any(
        |e| matches!(e, ServerMessage::SessionUpdated { session: s } if &s.id == session && s.agent == "default")
    ));

    send(
        &mut socket,
        1,
        ClientCommand::SessionAttach {
            session: session.clone(),
            since: None,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let message = events
        .iter()
        .find_map(|e| match e {
            ServerMessage::MessageDone { message, .. } if message.source.is_some() => Some(message),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        message.source,
        Some(MessageSource::Reminder {
            url: "site://plans/reminders/review.md".into()
        })
    );
    assert!(
        message
            .content
            .starts_with("Review the tickets (due 2099-10-19 08:00)"),
        "{}",
        message.content
    );
}

#[tokio::test]
async fn a_reminder_comes_back_to_the_session_that_wrote_it() {
    let page = "---\ntitle: Start ticket 0002\ntype: reminder\ndue: 2099-10-19\ntime: \"20:05\"\nstatus: open\naction: notify\n---\n";
    let (dir, server, _p, addr) = start(vec![
        // Turn 1: the agent writes the reminder.
        vec![finished(vec![tool_call(
            "w",
            "write",
            serde_json::json!({ "url": "site://plans/reminders/start.md", "content": page }),
        )])],
        vec![ModelEvent::Content("Noted.".into()), finished(vec![])],
        // Turn 2: the reminder arrives.
        vec![ModelEvent::Content("Starting it.".into()), finished(vec![])],
    ])
    .await;
    let mut socket = connect(addr, &server.token).await;
    send(
        &mut socket,
        1,
        ClientCommand::SessionCreate {
            agent: "default".into(),
            project: None,
        },
    )
    .await;
    let reply = until(&mut socket, |e| {
        matches!(e, ServerMessage::Ok { id: 1, .. })
    })
    .await;
    let Some(ServerMessage::Ok {
        result: composer_protocol::CommandResult::Session { session },
        ..
    }) = reply.last()
    else {
        panic!()
    };
    let id = session.id.clone();
    send(
        &mut socket,
        2,
        ClientCommand::SessionAttach {
            session: id.clone(),
            since: None,
        },
    )
    .await;
    send(
        &mut socket,
        3,
        ClientCommand::MessageSend {
            session: id.clone(),
            text: "remind me at 20:05".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(events.iter().any(|e| matches!(e, ServerMessage::ToolResult { result, .. } if result.contains("session:` set to this session"))));
    let written =
        std::fs::read_to_string(dir.path().join("sites/plans/reminders/start.md")).unwrap();
    assert!(
        written.contains(&format!("action: notify\nsession: {id}\n---")),
        "{written}"
    );

    let delivered = check_reminders(&server, at("2099-10-19 20:05"));
    assert_eq!(delivered.len(), 1);
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::ReminderDue { session: Some(s), .. } if *s == id))
    );
    assert!(events.iter().any(|e| matches!(e, ServerMessage::MessageDone { message, .. }
        if matches!(message.source, Some(MessageSource::Reminder { .. })) && message.content.starts_with("Start ticket 0002"))));
}

#[tokio::test]
async fn a_reminder_can_name_the_agent_for_its_session() {
    let (_dir, server, _p, addr) = start(vec![vec![
        ModelEvent::Content("Spawning them now.".into()),
        finished(vec![]),
    ]])
    .await;
    let url = SiteUrl::new("plans", "reminders/spawn.md").unwrap();
    server
        .write_page(
            &url,
            "---\ntitle: Spawn the tickets\ntype: reminder\ndue: 2099-10-19 08:00\nagent: orchestrator\n---\n",
            ChangedBy::Agent,
        )
        .unwrap();
    let mut socket = connect(addr, &server.token).await;

    check_reminders(&server, at("2099-10-19 09:00"));
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ReminderDue { .. })
    })
    .await;
    let ServerMessage::ReminderDue {
        session: Some(session),
        ..
    } = events.last().unwrap()
    else {
        panic!("{events:?}")
    };
    // The new session runs as the named agent, not default.
    assert!(events.iter().any(
        |e| matches!(e, ServerMessage::SessionUpdated { session: s } if &s.id == session && s.agent == "orchestrator")
    ),"{events:?}");
    assert_eq!(
        server.sessions.get(session).unwrap().info().agent,
        "orchestrator"
    );
}

#[tokio::test]
async fn an_unknown_agent_falls_back_to_default_with_a_note() {
    let (_dir, server, _p, addr) = start(vec![vec![
        ModelEvent::Content("I cannot spawn, but noted.".into()),
        finished(vec![]),
    ]])
    .await;
    let url = SiteUrl::new("plans", "reminders/water-ideas.md").unwrap();
    server
        .write_page(
            &url,
            "---\ntitle: Water the ideas\ntype: reminder\ndue: 2099-10-19 08:00\nagent: gardener\n---\n",
            ChangedBy::Agent,
        )
        .unwrap();
    let mut socket = connect(addr, &server.token).await;

    check_reminders(&server, at("2099-10-19 09:00"));
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ReminderDue { .. })
    })
    .await;
    let ServerMessage::ReminderDue {
        session: Some(session),
        ..
    } = events.last().unwrap()
    else {
        panic!("{events:?}")
    };
    // Not found: delivered anyway, in a default session.
    assert!(events.iter().any(
        |e| matches!(e, ServerMessage::SessionUpdated { session: s } if &s.id == session && s.agent == "default")
    ),"{events:?}");
    assert!(
        server
            .sessions
            .get(session)
            .unwrap()
            .info()
            .agent
            .starts_with("default"),
        "delivered, but to the wrong seat"
    );

    send(
        &mut socket,
        1,
        ClientCommand::SessionAttach {
            session: session.clone(),
            since: None,
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    let message = events
        .iter()
        .find_map(|e| match e {
            ServerMessage::MessageDone { message, .. } if message.source.is_some() => Some(message),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        message.source,
        Some(MessageSource::Reminder {
            url: "site://plans/reminders/water-ideas.md".into()
        })
    );
    assert!(
        message
            .content
            .starts_with("Water the ideas (due 2099-10-19 08:00)"),
        "{}",
        message.content
    );
    assert!(
        message
            .content
            .contains("note: agent gardener was not found"),
        "the delivered message must carry the note: {}",
        message.content
    );
}
