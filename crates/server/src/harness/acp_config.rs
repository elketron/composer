use std::collections::BTreeMap;

use composer_protocol::RunKind;
use serde::{Deserialize, Serialize};

use crate::config::Config;

/// An outside agent over the Agent Client Protocol, `[acp.<name>]`: the program composer starts
/// for a session whose `harness` names it. Only the user writes it: config.toml is outside the
/// web, where agents cannot write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcpConfig {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Added to composer's own environment.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl AcpConfig {
    /// The harness a run of `kind` asks for, checked: `None` (or blank) is composer's own; a name
    /// must be an `[acp.<name>]` of `config`, and only an agentic run may have one.
    pub fn check(
        config: &Config,
        kind: RunKind,
        harness: Option<&str>,
    ) -> Result<Option<String>, String> {
        let Some(name) = harness.map(str::trim).filter(|h| !h.is_empty()) else {
            return Ok(None);
        };
        if kind != RunKind::Agentic {
            return Err(format!(
                "an outside agent (harness `{name}`) runs agentic sessions only, not {}",
                kind_name(kind)
            ));
        }
        if !config.acp.contains_key(name) {
            let known: Vec<&str> = config.acp.keys().map(String::as_str).collect();
            return Err(match known.is_empty() {
                true => format!("no harness `{name}`: config.toml has no [acp.<name>] agents"),
                false => format!("no harness `{name}`; config.toml has {}", known.join(", ")),
            });
        }
        Ok(Some(name.to_string()))
    }
}

fn kind_name(kind: RunKind) -> &'static str {
    match kind {
        RunKind::SingleShot => "a single shot",
        RunKind::Workflow => "a workflow run",
        RunKind::Agentic => "an agentic run",
    }
}
