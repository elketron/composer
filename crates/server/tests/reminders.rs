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
    let (dir, server, _p, addr) = start(vec![]).await;
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
    assert_eq!(
        event.last().unwrap(),
        &ServerMessage::ReminderDue {
            url: "site://plans/reminders/call.md".into(),
            title: "Call the printer shop".into(),
            due: "2099-10-19 20:00".into(),
            late: false,
            session: None,
        }
    );
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
            "---\ntitle: Review the tickets\ntype: reminder\ndue: 2099-10-19 08:00\naction: agent\n---\n",
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
