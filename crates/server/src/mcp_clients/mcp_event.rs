use serde_json::Value;

/// The notification an MCP server sends to tell a session something the user did in its app.
pub const EVENT_METHOD: &str = "notifications/composer/event";

/// The `_meta` key on every `tools/call`: the id of the session that called.
pub const SESSION_META: &str = "composer/session";

/// What one event's text and its data each get, at most, in the notice.
const MAX_PART: usize = 4 * 1024;

/// A batch's notice carries this many events; the earlier ones are counted, not shown.
const MAX_BATCH: usize = 20;

/// One `notifications/composer/event`: `{ "session"?: string, "text": string, "data"?: any }`.
#[derive(Debug, Clone, PartialEq)]
pub struct McpEvent {
    /// The session it is for; none: the one that called the server last.
    pub session: Option<String>,
    pub text: String,
    pub data: Option<Value>,
}

impl McpEvent {
    /// The event in a notification's params, or why it is not one.
    pub fn parse(params: &Value) -> Result<Self, String> {
        let session = match params.get("session") {
            None | Some(Value::Null) => None,
            Some(Value::String(session)) => Some(session.clone()),
            Some(_) => return Err("`session` is not a string".into()),
        };
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or("it has no `text`")?
            .to_string();
        let data = params.get("data").filter(|data| !data.is_null()).cloned();
        Ok(Self {
            session,
            text,
            data,
        })
    }

    /// The message a session gets for `events` (in the order they came) from `server`:
    /// `[automatic notice: canvas event] text`, its data compact below it; several are listed.
    pub fn notice(server: &str, events: &[McpEvent]) -> String {
        match events {
            [one] => format!(
                "[automatic notice: {server} event] {}",
                one.body().replace('\n', "\n  ")
            ),
            _ => {
                let mut text = format!("[automatic notice: {} {server} events]", events.len());
                let skipped = events.len().saturating_sub(MAX_BATCH);
                if skipped > 0 {
                    text.push_str(&format!("\n({skipped} earlier ones left out)"));
                }
                for event in &events[skipped..] {
                    text.push_str("\n- ");
                    text.push_str(&event.body().replace('\n', "\n  "));
                }
                text
            }
        }
    }

    /// Its text, and its data as compact JSON on the next line.
    fn body(&self) -> String {
        let mut body = cut(&self.text, "text");
        if let Some(data) = &self.data {
            body.push('\n');
            body.push_str(&cut(&data.to_string(), "data"));
        }
        body
    }
}

/// `text`, cut at [`MAX_PART`] with a note saying `what` was cut.
fn cut(text: &str, what: &str) -> String {
    if text.len() <= MAX_PART {
        return text.to_string();
    }
    let mut end = MAX_PART;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (cut: the {what} was over 4 KB)", &text[..end])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_and_reads() {
        let event =
            McpEvent::parse(&json!({ "text": "clicked Save", "data": { "id": 3 } })).unwrap();
        assert_eq!(event.session, None);
        assert_eq!(
            McpEvent::notice("canvas", std::slice::from_ref(&event)),
            "[automatic notice: canvas event] clicked Save\n  {\"id\":3}"
        );
        let named = McpEvent::parse(&json!({ "session": "s1", "text": "hi" })).unwrap();
        assert_eq!(named.session.as_deref(), Some("s1"));
        assert_eq!(
            McpEvent::notice("canvas", &[event, named]),
            "[automatic notice: 2 canvas events]\n- clicked Save\n  {\"id\":3}\n- hi"
        );
        assert!(McpEvent::parse(&json!({ "data": 1 })).is_err());
        assert!(McpEvent::parse(&json!({ "text": "x", "session": 4 })).is_err());
    }

    #[test]
    fn big_data_is_cut() {
        let event = McpEvent::parse(&json!({ "text": "t", "data": "x".repeat(10_000) })).unwrap();
        let notice = McpEvent::notice("canvas", &[event]);
        assert!(notice.len() < 4300, "{}", notice.len());
        assert!(notice.ends_with("(cut: the data was over 4 KB)"));
    }

    #[test]
    fn a_long_batch_shows_its_last_ones() {
        let events: Vec<McpEvent> = (0..25)
            .map(|i| McpEvent::parse(&json!({ "text": format!("e{i}") })).unwrap())
            .collect();
        let notice = McpEvent::notice("canvas", &events);
        assert!(notice.starts_with(
            "[automatic notice: 25 canvas events]\n(5 earlier ones left out)\n- e5\n"
        ));
        assert!(notice.ends_with("- e24"));
    }
}
