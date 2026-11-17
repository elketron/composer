//! Tool-call arguments as a local model's chat template sometimes garbles them, put back the way
//! the tool's schema says, before any tool reads them. The transcripts up to 2026-11-17 show
//! three shapes, about 0.6% of all calls, each of which cost a step:
//!
//! - its own XML call format leaking into the JSON: `"site": "coding>\n<parameter=recipe>\nsh"`,
//!   or a key `"recipe\n</parameter"`;
//! - the arguments wrapped once more: `{"arguments": "{\"site\": …}"}`;
//! - booleans and numbers as strings: `"replace_all": "False"`.
//!
//! What a tool's own call shapes need (like `run`'s `action` for `recipe`) stays in that tool.

use serde_json::{Map, Value};

/// A call's arguments after repair, and the keys its tool doesn't take (said back when the call
/// fails, so the model sees why).
#[derive(Debug, Clone, PartialEq)]
pub struct RepairedArgs {
    pub args: Value,
    pub unknown: Vec<String>,
}

const LEAK: &str = "<parameter=";
const LEAK_END: &str = "</parameter>";

impl RepairedArgs {
    /// Repairs `args` for a tool whose JSON schema is `schema`.
    pub fn new(args: Value, schema: &Value) -> Self {
        let Value::Object(call) = args else {
            return Self {
                args,
                unknown: vec![],
            };
        };
        let properties = schema["properties"].as_object();
        let takes = |key: &str| properties.is_some_and(|p| p.contains_key(key));
        let mut call = unwrap_arguments(call, takes("arguments"));
        call = split_leaks(call, &takes);
        if let Some(properties) = properties {
            for (key, value) in call.iter_mut() {
                if let Some(kind) = properties.get(key).and_then(|p| p["type"].as_str()) {
                    coerce(value, kind);
                }
            }
        }
        let unknown = match properties {
            Some(_) => call.keys().filter(|k| !takes(k)).cloned().collect(),
            None => vec![],
        };
        Self {
            args: Value::Object(call),
            unknown,
        }
    }

    /// The note added to a failed call's result: which keys were not the tool's, and its own.
    pub fn unknown_note(&self, tool: &str, schema: &Value) -> Option<String> {
        if self.unknown.is_empty() {
            return None;
        }
        let names = |keys: Vec<&String>| {
            keys.iter()
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let takes = schema["properties"]
            .as_object()
            .map(|p| names(p.keys().collect()))
            .unwrap_or_default();
        Some(format!(
            "(not arguments of `{tool}`, ignored: {}; it takes {takes})",
            names(self.unknown.iter().collect())
        ))
    }
}

/// `{"arguments": "<the real arguments as JSON>"}` (or as an object): its keys, where the call
/// doesn't already have them. A wrapper that doesn't parse is left as it is.
fn unwrap_arguments(mut call: Map<String, Value>, takes_arguments: bool) -> Map<String, Value> {
    if takes_arguments {
        return call;
    }
    let inner = match call.get("arguments") {
        Some(Value::String(text)) => serde_json::from_str::<Value>(text).ok(),
        Some(object @ Value::Object(_)) => Some(object.clone()),
        _ => None,
    };
    if let Some(Value::Object(inner)) = inner {
        call.remove("arguments");
        for (key, value) in inner {
            call.entry(key).or_insert(value);
        }
    }
    call
}

/// Keys and string values with the template's `<parameter=name>` markup in them, split into the
/// keys they meant. Only markup at a value's edge that names one of the tool's own arguments
/// counts: a page or a script that talks about `<parameter=` is left alone.
fn split_leaks(call: Map<String, Value>, takes: &dyn Fn(&str) -> bool) -> Map<String, Value> {
    let mut clean = Map::new();
    let mut leaked = Vec::new();
    for (key, value) in call {
        let key = match clean_key(&key) {
            cleaned if !takes(&key) && takes(&cleaned) => cleaned,
            _ => key,
        };
        match value {
            Value::String(text) if is_leak(&text, takes) => {
                let (own, others) = split_value(&text);
                let own = own.or_else(|| {
                    others
                        .iter()
                        .find(|(name, _)| *name == key)
                        .map(|(_, v)| v.clone())
                });
                if let Some(own) = own {
                    clean.insert(key.clone(), Value::String(own));
                }
                leaked.extend(others.into_iter().filter(|(name, _)| *name != key));
            }
            value => {
                clean.insert(key, value);
            }
        }
    }
    for (name, value) in leaked {
        clean.entry(name).or_insert(Value::String(value));
    }
    clean
}

/// Markup where the template's leaks put it: right at the start, or after one short line
/// (`coding>\n<parameter=recipe>…`), naming an argument of the tool; or a closing tag after one
/// short line.
fn is_leak(text: &str, takes: &dyn Fn(&str) -> bool) -> bool {
    let one_short_line = |before: &str| before.len() <= 200 && !before.trim().contains('\n');
    if let Some(at) = text.find(LEAK) {
        let named = text[at + LEAK.len()..]
            .split('>')
            .next()
            .is_some_and(|name| takes(&clean_key(name)));
        return named && one_short_line(&text[..at]);
    }
    text.trim_end()
        .strip_suffix(LEAK_END)
        .is_some_and(one_short_line)
}

/// `"recipe\n</parameter"` → `recipe`.
fn clean_key(key: &str) -> String {
    key.split(|c: char| c.is_whitespace() || c == '<' || c == '>')
        .find(|part| !part.is_empty())
        .unwrap_or("")
        .to_string()
}

/// `"coding>\n<parameter=recipe>\nsh"` → own value `coding`, and `recipe` = `sh`.
fn split_value(text: &str) -> (Option<String>, Vec<(String, String)>) {
    let mut parts = text.split(LEAK);
    let own = parts.next().map(trim_markup).filter(|own| !own.is_empty());
    let others = parts
        .filter_map(|part| {
            let (name, value) = part.split_once('>')?;
            Some((clean_key(name), trim_markup(value)))
        })
        .filter(|(name, _)| !name.is_empty())
        .collect();
    (own, others)
}

/// A value without the markup around it: a closing `</parameter>`, a stray `>`, the newlines the
/// template puts around values.
fn trim_markup(text: &str) -> String {
    let mut text = text.trim();
    while let Some(rest) = text
        .strip_suffix(LEAK_END)
        .or_else(|| text.strip_suffix("</parameter"))
        .or_else(|| text.strip_suffix('>'))
    {
        text = rest.trim();
    }
    text.to_string()
}

/// A string where the schema wants a boolean or a number, when it reads as one.
fn coerce(value: &mut Value, kind: &str) {
    let Value::String(text) = value else {
        return;
    };
    let text = text.trim();
    let coerced = match kind {
        "boolean" => match text.to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        "integer" => text.parse::<i64>().ok().map(Value::from),
        "number" => text.parse::<f64>().ok().map(Value::from),
        _ => None,
    };
    if let Some(coerced) = coerced {
        *value = coerced;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "site": { "type": "string" },
                "recipe": { "type": "string" },
                "args": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["site", "recipe"]
        })
    }

    fn repaired(args: Value) -> RepairedArgs {
        RepairedArgs::new(args, &run_schema())
    }

    // The calls below are from the transcripts, shortened.

    #[test]
    fn a_parameter_leaking_into_another_value_becomes_its_own_key() {
        let fixed = repaired(json!({ "site": "coding>\n<parameter=recipe>\nsh", "args": ["ls"] }));
        assert_eq!(
            fixed.args,
            json!({ "site": "coding", "recipe": "sh", "args": ["ls"] })
        );
        assert!(fixed.unknown.is_empty());
    }

    #[test]
    fn a_value_that_is_only_its_own_markup_keeps_the_value() {
        let fixed = repaired(json!({ "recipe": "<parameter=recipe>\nsh", "site": "coding" }));
        assert_eq!(fixed.args, json!({ "recipe": "sh", "site": "coding" }));
    }

    #[test]
    fn a_key_with_markup_in_it_is_cleaned() {
        let fixed = repaired(json!({
            "args": ["sed -n '1,9p' a.rs"],
            "recipe\n</parameter": "<parameter=recipe>\nsh",
            "site": "coding"
        }));
        assert_eq!(
            fixed.args,
            json!({ "args": ["sed -n '1,9p' a.rs"], "recipe": "sh", "site": "coding" })
        );
    }

    #[test]
    fn a_value_naming_another_key_moves_there_and_the_lost_one_stays_missing() {
        let fixed = repaired(json!({ "args": ["cat a"], "recipe": "<parameter=site>\ncoding" }));
        assert_eq!(fixed.args, json!({ "args": ["cat a"], "site": "coding" }));
    }

    #[test]
    fn a_closing_tag_after_a_value_is_dropped() {
        let fixed = repaired(json!({ "site": "coding\n</parameter>", "recipe": "sh" }));
        assert_eq!(fixed.args, json!({ "site": "coding", "recipe": "sh" }));
    }

    #[test]
    fn arguments_wrapped_in_arguments_are_unwrapped() {
        let fixed = repaired(json!({
            "arguments": "{\"site\": \"coding\", \"recipe\": \"sh\", \"args\": [\"git status\"]}"
        }));
        assert_eq!(
            fixed.args,
            json!({ "site": "coding", "recipe": "sh", "args": ["git status"] })
        );
        assert!(fixed.unknown.is_empty());
    }

    #[test]
    fn a_wrapper_that_does_not_parse_stays_and_is_named_unknown() {
        let fixed = repaired(json!({
            "arguments": "[\"ls /; echo hi",
            "recipe": "sh",
            "site": "coding"
        }));
        assert_eq!(fixed.unknown, ["arguments"]);
        assert_eq!(
            fixed.unknown_note("run", &run_schema()).unwrap(),
            "(not arguments of `run`, ignored: `arguments`; it takes `site`, `recipe`, `args`)"
        );
    }

    #[test]
    fn booleans_and_numbers_as_strings_follow_the_schema() {
        let schema = json!({ "properties": {
            "replace_all": { "type": "boolean" },
            "compact": { "type": "boolean" },
            "lines": { "type": "integer" },
            "old_text": { "type": "string" }
        }});
        let fixed = RepairedArgs::new(
            json!({ "replace_all": "False", "compact": "true", "lines": "40", "old_text": "True" }),
            &schema,
        );
        assert_eq!(
            fixed.args,
            json!({ "replace_all": false, "compact": true, "lines": 40, "old_text": "True" })
        );
    }

    #[test]
    fn markup_inside_longer_text_is_left_alone() {
        let schema = json!({ "properties": {
            "url": { "type": "string" },
            "content": { "type": "string" }
        }});
        let page = "# Leaks\n\nThe template writes `<parameter=url>` then the value.\n</parameter>";
        let call = json!({ "url": "site://notes/leaks.md", "content": page });
        assert_eq!(RepairedArgs::new(call.clone(), &schema).args, call);
        // Naming something the tool doesn't take is not a leak either.
        let call = json!({ "url": "<parameter=colour>\nred" });
        assert_eq!(RepairedArgs::new(call.clone(), &schema).args, call);
    }

    #[test]
    fn a_clean_call_is_left_as_it_is() {
        let call = json!({ "site": "coding", "recipe": "sh", "args": ["echo <b>hi</b>"] });
        assert_eq!(repaired(call.clone()).args, call);
    }
}
