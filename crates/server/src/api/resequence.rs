use composer_protocol::{Seq, ServerMessage};

/// The same event with another `seq`, or `None` for events without one.
pub fn resequence(event: &ServerMessage, seq: Seq) -> Option<ServerMessage> {
    let mut value = serde_json::to_value(event).ok()?;
    let object = value.as_object_mut()?;
    if !object.contains_key("seq") {
        return None;
    }
    object.insert("seq".into(), seq.into());
    serde_json::from_value(value).ok()
}
