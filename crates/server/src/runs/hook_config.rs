use composer_protocol::RunKind;
use serde::{Deserialize, Serialize};

use crate::secret::secret_eq;

/// A webhook, `[hooks.<name>]`: `POST /api/hooks/<name>` with this hook's bearer token starts a
/// run of `kind` and answers with its result. Only the user writes it: config.toml is outside
/// the web, where agents cannot write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookConfig {
    /// A file holding the hook's bearer token (`~/` expanded), read and trimmed at every call.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_file: String,
    /// The bearer token itself, when there is no `token_file`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    pub kind: RunKind,
    /// The site the run is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    /// A workflow folder inside the site, whose instructions (`CONTEXT.md`, else `index.md`)
    /// the run follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The run's input, filled from the JSON body: `{{body}}` is the whole body (a string body
    /// as itself), `{{field}}` one of its top-level fields. Empty: the body.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,
    /// Store the run as a session (its id comes back too). Off: nothing is written.
    #[serde(default)]
    pub store: bool,
    /// The run is stopped after this many seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    300
}

impl HookConfig {
    /// Whether `presented` is this hook's token (compared in constant time). A hook whose token
    /// is empty or cannot be read accepts none.
    pub fn accepts(&self, presented: &str) -> bool {
        let token = if self.token_file.is_empty() {
            self.token.trim().to_string()
        } else {
            let path = crate::project::expand_home(&self.token_file);
            match std::fs::read_to_string(&path) {
                Ok(text) => text.trim().to_string(),
                Err(e) => {
                    tracing::warn!("hook token file {}: {e}", path.display());
                    return false;
                }
            }
        };
        !token.is_empty() && secret_eq(presented, &token)
    }

    /// The run's input from the request `body`: the `prompt` template filled in.
    pub fn input(&self, body: &serde_json::Value) -> Result<String, String> {
        let text = |value: &serde_json::Value| match value {
            serde_json::Value::String(s) => s.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_default(),
        };
        if self.prompt.is_empty() {
            return Ok(text(body));
        }
        let mut input = String::new();
        let mut rest = self.prompt.as_str();
        while let Some(start) = rest.find("{{") {
            input.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                input.push_str(&rest[start..]);
                rest = "";
                break;
            };
            let name = after[..end].trim();
            if name == "body" {
                input.push_str(&text(body));
            } else {
                let value = body
                    .get(name)
                    .ok_or_else(|| format!("the body has no field `{name}`"))?;
                input.push_str(&text(value));
            }
            rest = &after[end + 2..];
        }
        input.push_str(rest);
        Ok(input)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn hook(prompt: &str) -> HookConfig {
        HookConfig {
            token_file: String::new(),
            token: "secret".into(),
            kind: RunKind::SingleShot,
            site: None,
            folder: None,
            prompt: prompt.into(),
            store: false,
            timeout_secs: 300,
        }
    }

    #[test]
    fn the_template_takes_the_body_and_its_fields() {
        let body = json!({ "prompt": "a cat", "n": 2 });
        assert_eq!(
            hook("Improve: {{prompt}} ({{ n }})").input(&body).unwrap(),
            "Improve: a cat (2)"
        );
        assert_eq!(hook("").input(&json!("plain")).unwrap(), "plain");
        assert!(hook("{{body}}").input(&body).unwrap().contains("\"n\": 2"));
        assert!(hook("{{missing}}").input(&body).is_err());
        assert_eq!(hook("open {{").input(&body).unwrap(), "open {{");
    }

    #[test]
    fn only_its_own_token() {
        assert!(hook("").accepts("secret"));
        assert!(!hook("").accepts("secre"));
        assert!(
            !HookConfig {
                token: String::new(),
                ..hook("")
            }
            .accepts("")
        );
    }
}
