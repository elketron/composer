//! Pins the wire format: the clients depend on these exact shapes.

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
            source: MessageSource::Watch {
                pattern: "plans/**".into(),
            },
        },
        json!({ "type": "turn.queued", "session": "a3f1", "seq": 4, "source": { "kind": "watch", "pattern": "plans/**" } }),
    );
    round_trip(
        &ServerMessage::Ok {
            id: 9,
            result: CommandResult::Done,
        },
        json!({ "type": "ok", "id": 9, "result": { "kind": "done" } }),
    );
}

#[test]
fn background_processes() {
    round_trip(
        &ServerMessage::ProcessesChanged {
            session: "a3f1".into(),
            processes: vec![composer_protocol::ProcessInfo {
                terminal: "t1".into(),
                command: "npm run dev".into(),
                cwd: "/p".into(),
                started: "2026-11-21T12:00:00Z".into(),
            }],
        },
        json!({ "type": "processes.changed", "session": "a3f1", "processes": [
            { "terminal": "t1", "command": "npm run dev", "cwd": "/p", "started": "2026-11-21T12:00:00Z" }
        ] }),
    );
    round_trip(
        &MessageSource::Process {
            terminal: "t1".into(),
            command: "npm run dev".into(),
        },
        json!({ "kind": "process", "terminal": "t1", "command": "npm run dev" }),
    );
    round_trip(
        &ClientRequest {
            id: 3,
            command: ClientCommand::ProcessStop {
                terminal: "t1".into(),
            },
        },
        json!({ "id": 3, "type": "process.stop", "terminal": "t1" }),
    );
}
