use serde_json::Value;

use crate::provider::ModelToolCall;

/// Accumulates streamed `delta.tool_calls` fragments, keyed by their `index`.
#[derive(Debug, Default)]
pub struct PartialToolCalls {
    calls: Vec<ModelToolCall>,
}

impl PartialToolCalls {
    /// Applies one `delta.tool_calls` array.
    pub fn apply(&mut self, deltas: &[Value]) {
        for delta in deltas {
            let index = delta.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            while self.calls.len() <= index {
                self.calls.push(ModelToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                });
            }
            let call = &mut self.calls[index];
            if let Some(id) = delta.get("id").and_then(Value::as_str) {
                call.id = id.to_string();
            }
            if let Some(function) = delta.get("function") {
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    call.name.push_str(name);
                }
                if let Some(args) = function.get("arguments").and_then(Value::as_str) {
                    call.arguments.push_str(args);
                }
            }
        }
    }

    /// The completed calls. Calls without a name are dropped; missing ids are generated.
    pub fn finish(self) -> Vec<ModelToolCall> {
        self.calls
            .into_iter()
            .enumerate()
            .filter(|(_, c)| !c.name.is_empty())
            .map(|(i, mut c)| {
                if c.id.is_empty() {
                    c.id = format!("call_{i}");
                }
                if c.arguments.trim().is_empty() {
                    c.arguments = "{}".into();
                }
                c
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accumulates_fragments() {
        let mut p = PartialToolCalls::default();
        p.apply(&[json!({"index":0,"id":"c1","type":"function","function":{"name":"open","arguments":""}})]);
        p.apply(&[json!({"index":0,"function":{"arguments":"{\"url\":\"site://"}})]);
        p.apply(&[json!({"index":0,"function":{"arguments":"home\"}"}})]);
        p.apply(&[json!({"index":1,"function":{"name":"list_sites"}})]);
        let calls = p.finish();
        assert_eq!(calls[0].id, "c1");
        assert_eq!(calls[0].arguments, r#"{"url":"site://home"}"#);
        assert_eq!(calls[1].id, "call_1");
        assert_eq!(calls[1].arguments, "{}");
    }
}
