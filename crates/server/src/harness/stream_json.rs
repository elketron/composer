//! Claude Code's `--output-format stream-json` lines, reduced to what composer uses.

use serde_json::Value;

/// One thing that happened in a Claude Code run.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Init {
        session_id: String,
        model: String,
    },
    Text(String),
    Thinking(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },
    /// `rejected`: a usage limit was hit; `resets_at` in unix seconds.
    RateLimit {
        rejected: bool,
        resets_at: Option<i64>,
    },
    /// The run's end: `subtype` is `success`, `error_max_turns`, `error_during_execution`, …
    Result {
        subtype: String,
        is_error: bool,
        text: String,
        /// The API's HTTP status when a request failed (429: rate limited).
        api_error_status: Option<i64>,
    },
}

/// The events in one line; unknown or malformed lines give none.
pub fn parse_line(line: &str) -> Vec<StreamEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    match value["type"].as_str() {
        Some("system") if value["subtype"] == "init" => vec![StreamEvent::Init {
            session_id: text(&value["session_id"]),
            model: text(&value["model"]),
        }],
        Some("assistant") => blocks(&value)
            .filter_map(|block| match block["type"].as_str()? {
                "text" => Some(StreamEvent::Text(text(&block["text"]))),
                "thinking" => Some(StreamEvent::Thinking(text(&block["thinking"]))),
                "tool_use" => Some(StreamEvent::ToolUse {
                    id: text(&block["id"]),
                    name: text(&block["name"]),
                    input: block["input"].clone(),
                }),
                _ => None,
            })
            .filter(|e| !matches!(e, StreamEvent::Text(t) | StreamEvent::Thinking(t) if t.trim().is_empty()))
            .collect(),
        Some("user") => blocks(&value)
            .filter(|block| block["type"] == "tool_result")
            .map(|block| StreamEvent::ToolResult {
                id: text(&block["tool_use_id"]),
                output: match &block["content"] {
                    Value::String(s) => s.clone(),
                    Value::Array(parts) => parts
                        .iter()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                },
                is_error: block["is_error"].as_bool().unwrap_or(false),
            })
            .collect(),
        Some("rate_limit_event") => {
            let info = &value["rate_limit_info"];
            vec![StreamEvent::RateLimit {
                rejected: info["status"] == "rejected",
                resets_at: info["resetsAt"].as_i64(),
            }]
        }
        Some("result") => vec![StreamEvent::Result {
            subtype: text(&value["subtype"]),
            is_error: value["is_error"].as_bool().unwrap_or(false),
            text: text(&value["result"]),
            api_error_status: value["api_error_status"].as_i64(),
        }],
        _ => vec![],
    }
}

fn blocks(value: &Value) -> impl Iterator<Item = &Value> {
    value["message"]["content"].as_array().into_iter().flatten()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_run() {
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"6f1c","model":"claude-haiku-5-5","tools":[],"mcp_servers":[]}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"plan"},{"type":"text","text":"Reading the ticket."},{"type":"tool_use","id":"t1","name":"mcp__composer__open","input":{"url":"site://orchestration/tickets/0001"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"Ticket 0001"}]}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"no such file","is_error":true}]}}"#,
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1792513200}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"hi","api_error_status":null}"#,
        ];
        let events: Vec<StreamEvent> = lines.iter().flat_map(|l| parse_line(l)).collect();
        assert_eq!(
            events,
            vec![
                StreamEvent::Init {
                    session_id: "6f1c".into(),
                    model: "claude-haiku-5-5".into()
                },
                StreamEvent::Thinking("plan".into()),
                StreamEvent::Text("Reading the ticket.".into()),
                StreamEvent::ToolUse {
                    id: "t1".into(),
                    name: "mcp__composer__open".into(),
                    input: json!({ "url": "site://orchestration/tickets/0001" })
                },
                StreamEvent::ToolResult {
                    id: "t1".into(),
                    output: "Ticket 0001".into(),
                    is_error: false
                },
                StreamEvent::ToolResult {
                    id: "t2".into(),
                    output: "no such file".into(),
                    is_error: true
                },
                StreamEvent::RateLimit {
                    rejected: false,
                    resets_at: Some(1792513200)
                },
                StreamEvent::Result {
                    subtype: "success".into(),
                    is_error: false,
                    text: "hi".into(),
                    api_error_status: None
                },
            ]
        );
    }

    #[test]
    fn noise_is_ignored() {
        assert!(parse_line("not json").is_empty());
        assert!(parse_line(r#"{"type":"stream_event"}"#).is_empty());
        assert!(
            parse_line(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  "}]}}"#
            )
            .is_empty()
        );
    }
}
