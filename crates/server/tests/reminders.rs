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

#[tokio::test]
async fn replacing_an_existing_reminder_does_not_claim_it() {
    let page = "---\ntitle: y\ntype: reminder\ndue: 2099-12-31\nstatus: open\naction: notify\nsession:\n---\n# Y\n";
    let (dir, server, _p, addr) = start(vec![
        // Turn 1: the agent replaces the whole page; the blank session must stay blank.
        vec![finished(vec![tool_call(
            "w",
            "write",
            serde_json::json!({ "url": "site://plans/reminders/y.md", "content": page }),
        )])],
        vec![ModelEvent::Content("Rewritten.".into()), finished(vec![])],
    ])
    .await;
    let url = SiteUrl::new("plans", "reminders/y.md").unwrap();
    server.write_page(&url, page, ChangedBy::Agent).unwrap();
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
            session: id,
            text: "rewrite it".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::ToolResult { result, .. }
        if result.starts_with("replaced site://plans/reminders/y.md")
            && !result.contains("set to this session"))),
        "{events:?}"
    );
    let written = std::fs::read_to_string(dir.path().join("sites/plans/reminders/y.md")).unwrap();
    assert_eq!(written, page, "a replace must not stamp session:");
}

#[tokio::test]
async fn editing_a_reminders_body_does_not_claim_it() {
    let (dir, server, _p, addr) = start(vec![
        // Turn 1: the agent edits the heading; the blank session must stay blank.
        vec![finished(vec![tool_call(
            "e",
            "edit",
            serde_json::json!({ "url": "site://plans/reminders/z.md", "old_text": "# Z", "new_text": "# Zzz" }),
        )])],
        vec![ModelEvent::Content("Edited.".into()), finished(vec![])],
    ])
    .await;
    let url = SiteUrl::new("plans", "reminders/z.md").unwrap();
    server
        .write_page(
            &url,
            "---\ntitle: z\ntype: reminder\ndue: 2099-12-31\nstatus: open\naction: notify\nsession:\n---\n# Z\n",
            ChangedBy::Agent,
        )
        .unwrap();
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
            session: id,
            text: "rename it".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![],
        },
    )
    .await;
    let events = until(&mut socket, |e| matches!(e, ServerMessage::TurnDone { .. })).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ServerMessage::ToolResult { result, .. }
        if result.starts_with("edited site://plans/reminders/z.md")
            && !result.contains("set to this session"))),
        "{events:?}"
    );
    let written = std::fs::read_to_string(dir.path().join("sites/plans/reminders/z.md")).unwrap();
    assert!(written.contains("# Zzz"), "{written}");
    assert!(
        written.contains("session:\n---"),
        "an edit must not stamp session: {written}"
    );
}

#[tokio::test]
async fn a_recurring_reminder_rolls_on_and_stays_open() {
    let reply = || {
        vec![
            ModelEvent::Content("Done for today.".into()),
            finished(vec![]),
        ]
    };
    let (dir, server, _p, addr) = start(vec![reply(), reply(), reply()]).await;
    let url = SiteUrl::new("plans", "reminders/plants.md").unwrap();
    server
        .write_page(
            &url,
            "---\ntitle: Water the plants\ntype: reminder\ndue: 2099-10-19\ntime: \"20:00\"\nrecurs: daily\nstatus: open\n---\n# Water the plants\n\nThe ones on the balcony.\n",
            ChangedBy::Agent,
        )
        .unwrap();
    let path = dir.path().join("sites/plans/reminders/plants.md");
    let mut socket = connect(addr, &server.token).await;

    assert_eq!(check_reminders(&server, at("2099-10-19 20:00")).len(), 1);
    let page = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        page,
        "---\ntitle: Water the plants\ntype: reminder\ndue: 2099-10-20\ntime: \"20:00\"\nrecurs: daily\nstatus: open\n---\n# Water the plants\n\nThe ones on the balcony.\n\n- fired 2099-10-19 20:00\n"
    );
    let events = until(&mut socket, |e| {
        matches!(e, ServerMessage::ReminderDue { .. })
    })
    .await;
    let Some(ServerMessage::ReminderDue {
        due,
        session: Some(session),
        ..
    }) = events.last()
    else {
        panic!("{events:?}")
    };
    assert_eq!(due, "2099-10-19 20:00");
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
    assert!(
        message
            .content
            .contains("it recurs daily and is next due 2099-10-20 20:00, so leave its status open"),
        "{}",
        message.content
    );

    // Not again until tomorrow evening; then once more, and the history grows.
    assert!(check_reminders(&server, at("2099-10-20 19:59")).is_empty());
    assert_eq!(check_reminders(&server, at("2099-10-20 20:02")).len(), 1);
    let page = std::fs::read_to_string(&path).unwrap();
    assert!(page.contains("due: 2099-10-21\n"), "{page}");
    assert!(
        page.ends_with("balcony.\n\n- fired 2099-10-19 20:00\n- fired 2099-10-20 20:02\n"),
        "{page}"
    );

    // Dropping it stops it.
    server
        .write_page(
            &url,
            &page.replace("status: open", "status: done"),
            ChangedBy::External,
        )
        .unwrap();
    assert!(check_reminders(&server, at("2099-10-25 20:00")).is_empty());
}

/// `due:` owns the phase: the next firing counts on from the `due:` that fired, so moving
/// `due:` (to another day or time) moves the whole series; missed firings are skipped.
#[tokio::test]
async fn editing_due_on_a_recurring_reminder_resets_its_phase() {
    let reply = || vec![ModelEvent::Content("Ok.".into()), finished(vec![])];
    let (dir, server, _p, _addr) = start(vec![reply(), reply(), reply()]).await;
    let url = SiteUrl::new("plans", "reminders/standup.md").unwrap();
    let path = dir.path().join("sites/plans/reminders/standup.md");
    server
        .write_page(
            &url,
            "---\ntitle: Standup notes\ntype: reminder\ndue: 2099-10-19 09:00\nrecurs: weekly\n---\n",
            ChangedBy::Agent,
        )
        .unwrap();
    assert_eq!(check_reminders(&server, at("2099-10-19 09:00")).len(), 1);
    let page = std::fs::read_to_string(&path).unwrap();
    assert!(page.contains("due: 2099-10-26 09:00\n"), "{page}");

    // Moved to Wednesday afternoon: that is when it goes off, and a week on from there next.
    server
        .write_page(
            &url,
            &page.replace("due: 2099-10-26 09:00", "due: 2099-10-21 14:30"),
            ChangedBy::External,
        )
        .unwrap();
    assert!(check_reminders(&server, at("2099-10-21 14:29")).is_empty());
    assert_eq!(check_reminders(&server, at("2099-10-21 14:30")).len(), 1);
    let page = std::fs::read_to_string(&path).unwrap();
    assert!(page.contains("due: 2099-10-28 14:30\n"), "{page}");

    // Off for three weeks: one late delivery, and the series keeps its Wednesday 14:30.
    let delivered = check_reminders(&server, at("2099-11-15 10:00"));
    assert_eq!(delivered.len(), 1);
    let page = std::fs::read_to_string(&path).unwrap();
    assert!(page.contains("due: 2099-11-18 14:30\n"), "{page}");
    assert!(
        page.ends_with(
            "- fired 2099-10-19 09:00\n- fired 2099-10-21 14:30\n- fired 2099-11-15 10:00\n"
        ),
        "{page}"
    );
}

/// Recorded before delivered: a recurring reminder whose page can't be written goes nowhere
/// (no session, no event), and goes off once the page is writable again.
#[tokio::test]
async fn a_recurring_reminder_that_cant_roll_is_not_delivered() {
    use std::os::unix::fs::PermissionsExt;

    let reply = || vec![ModelEvent::Content("Ok.".into()), finished(vec![])];
    let (dir, server, _p, _addr) = start(vec![reply()]).await;
    let url = SiteUrl::new("plans", "reminders/backup.md").unwrap();
    let path = dir.path().join("sites/plans/reminders/backup.md");
    let page =
        "---\ntitle: Check the backup\ntype: reminder\ndue: 2099-10-19 07:00\nrecurs: daily\n---\n";
    server.write_page(&url, page, ChangedBy::Agent).unwrap();
    let sessions = server.sessions.list().len();

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let delivered = check_reminders(&server, at("2099-10-19 07:00"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(delivered.is_empty(), "{delivered:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), page);
    assert_eq!(server.sessions.list().len(), sessions, "no session started");

    assert_eq!(check_reminders(&server, at("2099-10-19 07:01")).len(), 1);
    let rolled = std::fs::read_to_string(&path).unwrap();
    assert!(rolled.contains("due: 2099-10-20 07:00\n"), "{rolled}");
    assert_eq!(server.sessions.list().len(), sessions + 1);
}
