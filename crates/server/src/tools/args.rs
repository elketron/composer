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
pub struct SetProjectArgs {
    /// Empty or missing clears the active project.
    pub path: Option<String>,
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
pub struct NewAppArgs {
    pub site: String,
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
pub struct NewProjectArgs {
    pub name: String,
    /// One line: what the project is.
    pub about: String,
    /// A site whose `templates/project/` folder the project starts from.
    pub template: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SpawnWorkerArgs {
    pub ticket: String,
    #[serde(default)]
    pub harness: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MergeTicketArgs {
    pub ticket: String,
    /// Target branch; defaults to the ticket's `merge_into:` frontmatter, else the project's
    /// checked-out branch.
    #[serde(default)]
    pub into: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnswerApprovalArgs {
    pub ticket: String,
    /// `approve`, `deny` or `ask_user`.
    pub decision: String,
    pub reason: String,
    #[serde(default)]
    pub request: Option<String>,
}

/// `continue_worker` and `stop_worker`.
#[derive(Debug, Deserialize)]
pub struct SteerWorkerArgs {
    pub ticket: String,
    pub message: String,
    /// `continue_worker` only: compact the worker's session first.
    #[serde(default, deserialize_with = "boolish")]
    pub compact: bool,
}

#[derive(Debug, Deserialize)]
pub struct FinishArgs {
    /// `done` or `failed`.
    pub status: String,
    pub summary: String,
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

#[derive(Debug, Deserialize)]
pub struct PublishSiteArgs {
    pub site: String,
    #[serde(default, rename = "as")]
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bools_from_models_are_forgiving() {
        let steer = |value: Value| {
            serde_json::from_value::<SteerWorkerArgs>(serde_json::json!({
                "ticket": "0001",
                "message": "go on",
                "compact": value,
            }))
            .unwrap()
            .compact
        };
        assert!(steer(Value::Bool(true)));
        assert!(steer(Value::String("True".into())));
        assert!(steer(Value::String("yes".into())));
        assert!(!steer(Value::String("False".into())));
    }
}
