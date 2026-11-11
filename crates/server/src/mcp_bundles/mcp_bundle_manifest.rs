use std::collections::BTreeMap;

use serde_json::Value;

/// A setting the user may give a bundle (`user_config` in the manifest).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UserSetting {
    pub default: Option<String>,
    pub required: bool,
}

/// What composer needs from a bundle's `manifest.json`: how to start the server on Linux.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct McpBundleManifest {
    pub name: String,
    /// `server.type`: `node`, `python`, `binary`, `uv`.
    pub server_type: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub user_config: BTreeMap<String, UserSetting>,
}

impl McpBundleManifest {
    /// The manifest in `text`, with `mcp_config.platform_overrides.linux` applied field by field.
    pub fn parse(text: &str) -> Result<Self, String> {
        let json: Value =
            serde_json::from_str(text).map_err(|e| format!("manifest.json is not JSON: {e}"))?;
        let server = &json["server"];
        let base = &server["mcp_config"];
        let linux = &base["platform_overrides"]["linux"];
        let pick = |field: &str| -> &Value {
            if linux.get(field).is_some_and(|v| !v.is_null()) {
                &linux[field]
            } else {
                &base[field]
            }
        };
        let command = pick("command").as_str().unwrap_or_default().to_string();
        if command.is_empty() {
            return Err("manifest.json has no `server.mcp_config.command`".into());
        }
        let args = pick("args")
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let env = pick("env")
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| Some((k.clone(), scalar(v)?)))
            .collect();
        let user_config = json["user_config"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, setting)| {
                let setting = UserSetting {
                    default: setting.get("default").and_then(scalar),
                    required: setting["required"].as_bool().unwrap_or(false),
                };
                (key.clone(), setting)
            })
            .collect();
        Ok(Self {
            name: json["name"].as_str().unwrap_or_default().to_string(),
            server_type: server["type"].as_str().unwrap_or_default().to_string(),
            command,
            args,
            env,
            user_config,
        })
    }
}

/// A string, number or boolean as text.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_override_replaces_field_by_field() {
        let m = McpBundleManifest::parse(
            r#"{ "name": "X", "server": { "type": "node", "mcp_config": {
                "command": "node", "args": ["a"], "env": { "K": "v" },
                "platform_overrides": { "linux": { "command": "nodejs", "args": ["b", "c"] } }
            } },
            "user_config": { "token": { "required": true }, "n": { "default": 3 } } }"#,
        )
        .unwrap();
        assert_eq!(m.command, "nodejs");
        assert_eq!(m.args, ["b", "c"]);
        assert_eq!(m.env["K"], "v");
        assert!(m.user_config["token"].required);
        assert_eq!(m.user_config["n"].default.as_deref(), Some("3"));
    }

    #[test]
    fn refuses_what_it_cannot_start() {
        assert!(McpBundleManifest::parse("nope").is_err());
        assert!(McpBundleManifest::parse(r#"{ "name": "x" }"#).is_err());
    }
}
