//! Tool argument shapes, as sent by the model.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// A bool as models send it: `true`, `"True"`, `"yes"`, `1`.
pub fn boolish<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Bool(value) => value,
        Value::String(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "true" | "yes" | "on" | "1"
        ),
        Value::Number(value) => value.as_i64().is_some_and(|value| value != 0),
        _ => false,
    })
}

#[derive(Debug, Deserialize)]
pub struct OpenArgs {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchArgs {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct WriteArgs {
    pub url: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct EditArgs {
    pub url: String,
    pub old_text: String,
    pub new_text: String,
    #[serde(default, deserialize_with = "boolish")]
    pub replace_all: bool,
    /// The page hash `open` printed; refuses a stale edit when it differs.
    #[serde(default)]
    pub if_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActionsArgs {
    /// Default: the site the agent entered.
    pub site: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunArgs {
    pub site: String,
    pub recipe: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewSiteArgs {
    pub name: String,
    /// Defaults to the name.
    pub title: Option<String>,
    /// One line: what the site is for.
    pub about: String,
}

#[derive(Debug, Deserialize)]
pub struct WatchArgs {
    /// Missing: list the session's watches.
    pub pattern: Option<String>,
    #[serde(default = "yes")]
    pub once: bool,
    #[serde(default)]
    pub stop: bool,
}

fn yes() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bools_from_models_are_forgiving() {
        let steer = |value: Value| {
            serde_json::from_value::<EditArgs>(serde_json::json!({
                "url": "site://a/b",
                "old_text": "x",
                "new_text": "y",
                "replace_all": value,
            }))
            .unwrap()
            .replace_all
        };
        assert!(steer(Value::Bool(true)));
        assert!(steer(Value::String("True".into())));
        assert!(steer(Value::String("yes".into())));
        assert!(!steer(Value::String("False".into())));
    }
}
