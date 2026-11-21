//! `site.set` over the socket: the user enters a site or leaves it. The reply is a `note` with
//! what happened; the session's new state arrives as `session.updated`.

mod support;

use composer_protocol::{ClientCommand, CommandResult, ServerMessage};
use support::*;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Sends `site.set` and returns the reply (`Ok` result or the error message) with the events
/// that came before it.
async fn site_set(
    socket: &mut Socket,
    id: u64,
    session: &str,
    site: Option<&str>,
) -> (Result<CommandResult, String>, Vec<ServerMessage>) {
    send(
        socket,
        id,
        ClientCommand::SiteSet {
            session: session.into(),
            site: site.map(Into::into),
        },
    )
    .await;
    let mut events = until(socket, |e| {
        matches!(e, ServerMessage::Ok { id: i, .. } | ServerMessage::Error { id: Some(i), .. } if *i == id)
    })
    .await;
    let reply = match events.pop().unwrap() {
        ServerMessage::Ok { result, .. } => Ok(result),
        ServerMessage::Error { message, .. } => Err(message),
        other => panic!("{other:?}"),
    };
    (reply, events)
}

fn note(reply: Result<CommandResult, String>) -> String {
    match reply {
        Ok(CommandResult::Note { text }) => text,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_user_enters_and_leaves_a_site() {
    let (dir, server, _provider, addr) = start(vec![]).await;
    let sites = dir.path().join("sites");
    write(
        &sites.join("plain/index.md"),
        "---\ntitle: Plain\n---\n# Plain\n",
    );
    write(
        &sites.join("game/index.md"),
        "---\ntitle: Game\nproject: required\n---\n# Game\n",
    );
    let session = server.sessions.create("default", None).unwrap();
    let id = session.id().to_string();
    let mut socket = connect(addr, &server.token).await;

    // Entered, and the session update goes out.
    let (reply, events) = site_set(&mut socket, 1, &id, Some("plain")).await;
    let text = note(reply);
    assert!(text.contains("Entered plain"), "{text}");
    assert_eq!(session.info().site.as_deref(), Some("plain"));
    // The session update is the existing broadcast, not a second path (it may overtake the reply).
    let seen = |e: &ServerMessage| matches!(e, ServerMessage::SessionUpdated { session: s } if s.site.as_deref() == Some("plain"));
    if !events.iter().any(seen) {
        until(&mut socket, seen).await;
    }

    // `None` leaves; again, nothing to leave.
    let (reply, _) = site_set(&mut socket, 2, &id, None).await;
    assert!(note(reply).contains("Left site plain"));
    assert_eq!(session.info().site, None);
    let (reply, _) = site_set(&mut socket, 3, &id, None).await;
    assert!(note(reply).contains("not in a site"));

    // Errors change nothing: an unknown site, a site that needs a project, an unknown session.
    let before = session.info().site;
    let (reply, _) = site_set(&mut socket, 6, &id, Some("nope")).await;
    let message = reply.unwrap_err();
    assert!(message.contains("nope"), "{message}");
    let (reply, _) = site_set(&mut socket, 7, &id, Some("game")).await;
    let message = reply.unwrap_err();
    assert!(message.contains("needs an active project"), "{message}");
    assert_eq!(session.info().site, before);
    let (reply, _) = site_set(&mut socket, 8, "no-such-session", Some("plain")).await;
    assert!(reply.is_err());

    // With a project the gated site opens.
    session.update_info(&server.hub, |info| {
        info.project = Some(dir.path().display().to_string())
    });
    let (reply, _) = site_set(&mut socket, 9, &id, Some("game")).await;
    assert!(note(reply).contains("Entered game"));
    assert_eq!(session.info().site.as_deref(), Some("game"));
}
