//! Pins the wire format: the TUI, apps and workers depend on these exact shapes.

use composer_protocol::{
    ClientCommand, ClientRequest, CommandResult, MessageSource, ServerMessage,
};
use serde_json::{Value, json};

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
    expected: Value,
) {
    assert_eq!(serde_json::to_value(value).unwrap(), expected);
    assert_eq!(&serde_json::from_value::<T>(expected).unwrap(), value);
}

#[test]
fn client_request_is_flat() {
    let req = ClientRequest {
        id: 7,
        command: ClientCommand::SessionAttach {
            session: "a3f1".into(),
            since: Some(42),
        },
    };
    round_trip(
        &req,
        json!({ "id": 7, "type": "session.attach", "session": "a3f1", "since": 42 }),
    );
}

#[test]
fn message_send_defaults_attachments() {
    let req: ClientRequest = serde_json::from_value(
        json!({ "id": 1, "type": "message.send", "session": "a3f1", "text": "hi" }),
    )
    .unwrap();
    assert_eq!(
        req.command,
        ClientCommand::MessageSend {
            session: "a3f1".into(),
            text: "hi".into(),
            pages: vec![],
            images: vec![],
            attachments: vec![]
        }
    );
}

#[test]
fn unit_command() {
    round_trip(
        &ClientRequest {
            id: 2,
            command: ClientCommand::SessionList,
        },
        json!({ "id": 2, "type": "session.list" }),
    );
}

#[test]
fn server_events() {
    round_trip(
        &ServerMessage::ThinkingDelta {
            session: "a3f1".into(),
            seq: 3,
            text: "hm".into(),
        },
        json!({ "type": "thinking.delta", "session": "a3f1", "seq": 3, "text": "hm" }),
    );
    round_trip(
        &ServerMessage::TurnQueued {
            session: "a3f1".into(),
            seq: 4,
            source: MessageSource::App {
                site: "design".into(),
            },
        },
        json!({ "type": "turn.queued", "session": "a3f1", "seq": 4, "source": { "kind": "app", "site": "design" } }),
    );
    round_trip(
        &ServerMessage::Ok {
            id: 9,
            result: CommandResult::Done,
        },
        json!({ "type": "ok", "id": 9, "result": { "kind": "done" } }),
    );
}
