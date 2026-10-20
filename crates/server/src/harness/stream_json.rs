//! Claude Code's `--output-format stream-json` lines, reduced to what composer uses.

use serde_json::Value;

/// One thing that happened in a Claude Code run.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Init {
        session_id: String,
        model: String,
    },
    /// A complete text block (deltas before it are superseded).
    Text(String),
    Thinking(String),
    /// Text as it streams (`--include-partial-messages`).
    TextDelta(String),
    ThinkingDelta(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
        /// The `Agent` call it was made in (a sub-agent's).
        parent: Option<String>,
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
        turns: Option<i64>,
        duration_ms: Option<i64>,
        /// What it would cost at API prices (a subscription isn't billed per run).
        cost_usd: Option<f64>,
    },
}

/// The events in one line; unknown or malformed lines give none.
pub fn parse_line(line: &str) -> Vec<StreamEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let parent = value["parent_tool_use_id"].as_str().map(str::to_string);
    match value["type"].as_str() {
        // A sub-agent's streaming and text stay inside it; its tool calls show (nested).
        Some("stream_event") if parent.is_none() => {
            let delta = &value["event"]["delta"];
            let event = match delta["type"].as_str() {
                Some("text_delta") => StreamEvent::TextDelta(text(&delta["text"])),
                Some("thinking_delta") => StreamEvent::ThinkingDelta(text(&delta["thinking"])),
                _ => return vec![],
            };
            match &event {
                StreamEvent::TextDelta(t) | StreamEvent::ThinkingDelta(t) if t.is_empty() => {
                    vec![]
                }
                _ => vec![event],
            }
        }
        Some("system") if value["subtype"] == "init" => vec![StreamEvent::Init {
            session_id: text(&value["session_id"]),
            model: text(&value["model"]),
        }],
        Some("assistant") => blocks(&value)
            .filter_map(|block| match block["type"].as_str()? {
                "text" if parent.is_none() => Some(StreamEvent::Text(text(&block["text"]))),
                "thinking" if parent.is_none() => {
                    Some(StreamEvent::Thinking(text(&block["thinking"])))
                }
                "tool_use" => Some(StreamEvent::ToolUse {
                    id: text(&block["id"]),
                    name: text(&block["name"]),
                    input: block["input"].clone(),
                    parent: parent.clone(),
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
            turns: value["num_turns"].as_i64(),
            duration_ms: value["duration_ms"].as_i64(),
            cost_usd: value["total_cost_usd"].as_f64(),
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
            r#"{"type":"result","subtype":"success","is_error":false,"result":"hi","api_error_status":null,"num_turns":9,"duration_ms":14123,"total_cost_usd":0.0079}"#,
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
                    input: json!({ "url": "site://orchestration/tickets/0001" }),
                    parent: None,
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
                    api_error_status: None,
                    turns: Some(9),
                    duration_ms: Some(14123),
                    cost_usd: Some(0.0079),
                },
            ]
        );
    }

    #[test]
    fn partial_messages_and_sub_agents() {
        let lines = [
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Do"}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{"}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"inner"}},"parent_tool_use_id":"a1"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"I'll read it."},{"type":"tool_use","id":"t9","name":"Read","input":{"file_path":"a.txt"}}]},"parent_tool_use_id":"a1"}"#,
        ];
        let events: Vec<StreamEvent> = lines.iter().flat_map(|l| parse_line(l)).collect();
        assert_eq!(
            events,
            vec![
                StreamEvent::TextDelta("Do".into()),
                StreamEvent::ToolUse {
                    id: "t9".into(),
                    name: "Read".into(),
                    input: json!({ "file_path": "a.txt" }),
                    parent: Some("a1".into()),
                },
            ]
        );
    }

    #[test]
    fn noise_is_ignored() {
        assert!(parse_line("not json").is_empty());
        assert!(
            parse_line(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#).is_empty()
        );
        assert!(
            parse_line(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  "}]}}"#
            )
            .is_empty()
        );
    }
}
